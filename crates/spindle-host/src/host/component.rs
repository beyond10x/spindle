//! One component of a system on its own, its bindings carried over TCP to the processes that run
//! the others.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ess_compiler::EssIr;
use spindle_capability::{Admission, Environment};
use spindle_channel::tcp::TcpChannel;
use spindle_channel::{Receiver, Router};
use spindle_telemetry::Counters;
use spindle_timer::Clock;

use super::{Deliveries, Host, HostError, Report};
use crate::admission::{Placements, Refusal};

/// The channel implementation a one-component host carries bindings on.
const TCP: &str = "tcp";

/// How long [`Host::connect`] waits between attempts while a peer is not listening yet.
const RETRY: Duration = Duration::from_millis(10);

impl Host {
    /// Admits the one component `component` of `ir` against `environment`, with what it needs
    /// when it runs on its own, and opens its store as [`Host::new`] does.
    ///
    /// Its event bindings are carried over `tcp`: nothing is connected yet. [`Host::peers`] names
    /// the components its events lead to, [`Host::connect`] links each, and [`Host::accept`] takes
    /// the links its peers open to it. Only the periodic bindings whose command it accepts run
    /// here.
    ///
    /// # Errors
    /// [`HostError::Refused`] naming what the component lacks; [`HostError::Unsupported`] when
    /// the specification declares no such component, when its channel is bound to anything but
    /// `tcp`, or as [`Host::new`] refuses.
    pub fn component(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
        component: &str,
    ) -> Result<Self, HostError> {
        let requirements = spindle_capability::derive(&ir, Some(component)).ok_or_else(|| {
            HostError::Unsupported(format!("{} declares no component {component}", ir.system()))
        })?;
        let placements = match spindle_capability::admit(&requirements, environment) {
            Admission::Admitted { bindings } => {
                Placements::from([(component.to_owned(), bindings)])
            }
            Admission::Refused { missing } => {
                return Err(HostError::Refused(Refusal {
                    environment: environment.name.clone(),
                    missing: BTreeMap::from([(component.to_owned(), missing)]),
                }));
            }
        };
        Self::assemble(ir, clock, state, placements, TCP)
    }

    /// Every component an event binding of a component running here leads to, with those
    /// bindings, excluding the components that run here.
    pub fn peers(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut peers = BTreeMap::<String, BTreeSet<String>>::new();
        let Ok(router) = Router::from_ir(&self.ir) else {
            return peers;
        };
        for route in router.routes() {
            if self.components.contains_key(&route.from) && !self.components.contains_key(&route.to)
            {
                peers
                    .entry(route.to.clone())
                    .or_default()
                    .insert(route.binding.clone());
            }
        }
        peers
    }

    /// Carries every event bound to `peer` over a TCP link to `address`, trying again every
    /// 10 ms while nothing listens there, for at most `within`.
    ///
    /// # Errors
    /// [`HostError::Channel`] with the last connection error once `within` has passed.
    pub fn connect(
        &mut self,
        peer: &str,
        address: SocketAddr,
        within: Duration,
    ) -> Result<(), HostError> {
        let deadline = Instant::now() + within;
        for component in self.components.values_mut() {
            let channel = loop {
                match TcpChannel::connect(address) {
                    Ok(channel) => break channel,
                    Err(error) if Instant::now() >= deadline => return Err(error.into()),
                    Err(_) => std::thread::sleep(RETRY),
                }
            };
            component.sender.connect(peer, channel);
        }
        Ok(())
    }

    /// Takes every link a peer has opened on `listener` and is waiting to be accepted; returns
    /// how many. Each carries envelopes to the component running here, which [`Host::deliver`]
    /// runs.
    ///
    /// # Errors
    /// [`HostError::Channel`] when the listener fails, or [`HostError::Unsupported`] when more
    /// than one component runs here, so an envelope could not be told where to go.
    pub fn accept(&mut self, listener: &TcpListener) -> Result<usize, HostError> {
        let [(name, component)] = self.components.iter().collect::<Vec<_>>()[..] else {
            return Err(HostError::Unsupported(
                "links are accepted only where one component runs".to_owned(),
            ));
        };
        let (name, sink) = (name.clone(), component.sink.clone());
        listener
            .set_nonblocking(true)
            .map_err(spindle_channel::ChannelError::from)?;
        let mut accepted = 0;
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .map_err(spindle_channel::ChannelError::from)?;
                    let channel = TcpChannel::new(stream)?;
                    self.receivers
                        .push(Receiver::new(&name, channel).with_telemetry(sink.clone()));
                    accepted += 1;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(accepted),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(spindle_channel::ChannelError::from(error).into()),
            }
        }
    }

    /// The links open toward the components running here. A link whose peer closed it is
    /// dropped the next time [`Host::deliver`] or [`Host::advance`] reads it.
    pub fn incoming(&self) -> usize {
        self.receivers.len()
    }

    /// Runs every envelope that has arrived, and everything those commands lead to, as
    /// [`Host::advance`] does after a tick.
    ///
    /// # Errors
    /// As [`Host::advance`].
    pub fn deliver(&mut self) -> Result<Report, HostError> {
        let mut report = Report::default();
        self.settle(&mut report, &mut Deliveries::default())?;
        Ok(report)
    }

    /// The counters of `component`, shared, for a telemetry sender; `None` when no such
    /// component runs here.
    pub fn shared_counters(&self, component: &str) -> Option<Arc<Counters>> {
        self.components
            .get(component)
            .map(|component| Arc::clone(&component.counters))
    }
}
