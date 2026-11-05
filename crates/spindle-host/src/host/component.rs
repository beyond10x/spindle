//! One component of a system on its own, its bindings carried over TCP links to the processes that
//! run the others.
//!
//! # Links
//!
//! A link is one TCP connection from the component that publishes to the component whose command a
//! binding invokes. Each message on it is a 4-byte big-endian length and that many bytes of JSON:
//!
//! | message | from | means |
//! |---|---|---|
//! | `{"hello": {"from", "to"}}` | the connecting end, first | the components the link joins |
//! | `{"frame": <Frame>}` | either end | a channel frame: an envelope, or an acknowledgement |
//! | `{"settled": 1}` | the accepting end | one envelope and everything it led to has run |
//! | `{"end": true}` | either end | the run is over; the link closes next |
//!
//! An accepted connection becomes a link only once its `hello` names this component and a peer
//! whose events lead here, once; any other is refused, and one that closes first changes nothing.
//! A link that closes, or fails, without an `end` is an error naming its peer.
//!
//! # Settling
//!
//! A call that runs a command here ([`Host::submit`], each tick of [`Host::advance`]) returns once
//! everything it led to has run on every component, as in one process: the host counts the
//! envelopes it sent that are not yet `settled`, and a component settles an envelope that found
//! it idle only once every envelope its commands sent has settled, and any other at once (the
//! termination detection of Dijkstra and Scholten). So a tick never starts while the last one's
//! consequences are still running elsewhere.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::{Admission, Environment};
use spindle_channel::{Channel, ChannelError, Frame, Receiver, Router};
use spindle_telemetry::Counters;
use spindle_timer::Clock;

use super::{Cause, DELIVERY_LIMIT, Deliveries, Host, HostError, Report};
use crate::admission::{Placements, Refusal};

/// The channel implementation a one-component host carries bindings on.
const TCP: &str = "tcp";
/// The largest message either end of a link sends or accepts.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;
/// How long one message may take to write.
const WRITE_WITHIN: Duration = Duration::from_secs(5);
/// How long an accepted connection may take to say `hello`.
const HELLO_WITHIN: Duration = Duration::from_secs(5);
/// How long a call waits for what it led to elsewhere to settle.
const SETTLE_WITHIN: Duration = Duration::from_secs(10);
/// How often a waiting call reads its links again.
const SETTLE_POLL: Duration = Duration::from_micros(200);

/// One end of a link.
struct Link {
    stream: TcpStream,
    buffer: Vec<u8>,
    frames: VecDeque<Frame>,
    hello: Option<(String, String)>,
    sent: u64,
    settled: u64,
    ended: bool,
    closed: Option<String>,
}

impl Link {
    fn new(stream: TcpStream) -> std::io::Result<Self> {
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            buffer: Vec::new(),
            frames: VecDeque::new(),
            hello: None,
            sent: 0,
            settled: 0,
            ended: false,
            closed: None,
        })
    }

    fn write(&mut self, message: &Value) -> Result<(), String> {
        if let Some(reason) = &self.closed {
            return Err(reason.clone());
        }
        let body = serde_json::to_vec(message).map_err(|error| error.to_string())?;
        let size = u32::try_from(body.len())
            .ok()
            .filter(|size| *size as usize <= MAX_MESSAGE)
            .ok_or_else(|| format!("a message of {} bytes is over the limit", body.len()))?;
        let mut bytes = size.to_be_bytes().to_vec();
        bytes.extend_from_slice(&body);
        let written = self
            .stream
            .set_nonblocking(false)
            .and_then(|()| self.stream.set_write_timeout(Some(WRITE_WITHIN)))
            .and_then(|()| self.stream.write_all(&bytes));
        let restored = self.stream.set_nonblocking(true);
        if let Err(error) = written.and(restored) {
            let reason = format!("failed a write: {error}");
            self.closed = Some(reason.clone());
            return Err(reason);
        }
        Ok(())
    }

    /// Reads what has arrived and takes every whole message from it.
    fn poll(&mut self) {
        let mut chunk = [0_u8; 8192];
        while self.closed.is_none() {
            match self.stream.read(&mut chunk) {
                Ok(0) => self.closed = Some("closed".to_owned()),
                Ok(read) => self.buffer.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => self.closed = Some(format!("failed a read: {error}")),
            }
        }
        while let Some(prefix) = self.buffer.first_chunk::<4>() {
            let size = u32::from_be_bytes(*prefix) as usize;
            if size > MAX_MESSAGE {
                self.closed = Some(format!("sent a message of {size} bytes, over the limit"));
                self.buffer.clear();
                return;
            }
            if self.buffer.len() < 4 + size {
                return;
            }
            let message = serde_json::from_slice::<Value>(&self.buffer[4..4 + size]);
            self.buffer.drain(..4 + size);
            if let Err(reason) = message
                .map_err(|error| error.to_string())
                .and_then(|message| self.take(message))
            {
                self.closed = Some(format!("sent a message this end does not take: {reason}"));
                self.buffer.clear();
                return;
            }
        }
    }

    fn take(&mut self, message: Value) -> Result<(), String> {
        let Value::Object(mut message) = message else {
            return Err("not an object".to_owned());
        };
        if let Some(hello) = message.remove("hello") {
            let field = |name: &str| {
                hello
                    .get(name)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("a hello without {name}"))
            };
            self.hello = Some((field("from")?, field("to")?));
        } else if let Some(frame) = message.remove("frame") {
            let frame = serde_json::from_value(frame).map_err(|error| error.to_string())?;
            self.frames.push_back(frame);
        } else if message.remove("settled").is_some() {
            self.settled += 1;
        } else if message.remove("end").is_some() {
            self.ended = true;
        } else {
            return Err(format!("{}", Value::Object(message)));
        }
        Ok(())
    }

    /// Why the link can no longer be used, unless its peer ended it.
    fn broken(&self) -> Option<&str> {
        match (&self.closed, self.ended) {
            (Some(reason), false) => Some(reason),
            _ => None,
        }
    }
}

type Shared = Arc<Mutex<Link>>;

fn held(link: &Shared) -> MutexGuard<'_, Link> {
    link.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A link as the [`Channel`] a [`spindle_channel::Sender`] or [`Receiver`] carries frames on.
struct LinkChannel(Shared);

impl Channel for LinkChannel {
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        let mut link = held(&self.0);
        let envelope = matches!(frame, Frame::Deliver(_));
        link.write(&json!({ "frame": frame }))
            .map_err(ChannelError::Broken)?;
        if envelope {
            link.sent += 1;
        }
        Ok(())
    }

    fn receive(&mut self, _wait: Duration) -> Result<Option<Frame>, ChannelError> {
        let mut link = held(&self.0);
        link.poll();
        if let Some(frame) = link.frames.pop_front() {
            return Ok(Some(frame));
        }
        match link.broken() {
            Some(reason) => Err(ChannelError::Broken(reason.to_owned())),
            None => Ok(None),
        }
    }
}

/// What a one-component host keeps about its links.
pub(super) struct Remote {
    me: String,
    /// The components whose events lead here: one incoming link from each.
    expected: BTreeSet<String>,
    outgoing: BTreeMap<String, Shared>,
    incoming: BTreeMap<String, (Shared, Receiver)>,
    pending: Vec<(Link, Instant)>,
    refused: Vec<String>,
    /// The peer whose envelope found this component idle, still to be settled.
    engaged: Option<String>,
    ended: bool,
    stop: Option<Box<dyn Fn() -> bool + Send>>,
}

impl Remote {
    fn deficit(&self) -> u64 {
        self.outgoing
            .values()
            .map(|link| {
                let link = held(link);
                link.sent.saturating_sub(link.settled)
            })
            .sum()
    }

    fn settle(&self, peer: &str) -> Result<(), HostError> {
        let Some((link, _)) = self.incoming.get(peer) else {
            return Ok(());
        };
        held(link)
            .write(&json!({ "settled": 1 }))
            .map_err(|reason| HostError::Peer {
                peer: peer.to_owned(),
                reason,
            })
    }

    /// Reads every link; a link that broke without an `end` is an error naming its peer.
    fn read(&mut self) -> Result<(), HostError> {
        let links = self
            .outgoing
            .iter()
            .chain(self.incoming.iter().map(|(peer, (link, _))| (peer, link)));
        for (peer, link) in links {
            let mut link = held(link);
            link.poll();
            self.ended |= link.ended;
            if let Some(reason) = link.broken() {
                return Err(HostError::Peer {
                    peer: peer.clone(),
                    reason: format!("{reason} before the run ended"),
                });
            }
        }
        Ok(())
    }
}

impl Host {
    /// Admits the one component `component` of `ir` against `environment`, with what it needs
    /// when it runs on its own, and opens its store as [`Host::new`] does.
    ///
    /// Its event bindings are carried over `tcp` links (see the module documentation): nothing
    /// is linked yet. [`Host::peers`] names the components its events lead to and
    /// [`Host::connect`] links each; [`Host::accept`] takes the links the components whose events
    /// lead here open, and [`Host::unlinked`] names those still missing. Only the periodic
    /// bindings whose command it accepts run here.
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
        let expected = Router::from_ir(&ir)?
            .routes()
            .iter()
            .filter(|route| route.to == component && route.from != component)
            .map(|route| route.from.clone())
            .collect();
        let mut host = Self::assemble(ir, clock, state, placements, TCP)?;
        host.remote = Some(Remote {
            me: component.to_owned(),
            expected,
            outgoing: BTreeMap::new(),
            incoming: BTreeMap::new(),
            pending: Vec::new(),
            refused: Vec::new(),
            engaged: None,
            ended: false,
            stop: None,
        });
        Ok(host)
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

    /// Makes a waiting call give up with [`HostError::Stopped`] once `stop` holds.
    pub fn stop_when(&mut self, stop: impl Fn() -> bool + Send + 'static) {
        if let Some(remote) = &mut self.remote {
            remote.stop = Some(Box::new(stop));
        }
    }

    /// Opens the link to `peer` at `address`, once, and says `hello`; every event bound to
    /// `peer` is carried on it.
    ///
    /// # Errors
    /// [`HostError::Peer`] when nothing listens there or the `hello` cannot be written;
    /// [`HostError::Unsupported`] on a host of every component.
    pub fn connect(&mut self, peer: &str, address: SocketAddr) -> Result<(), HostError> {
        let failed = |reason: String| HostError::Peer {
            peer: peer.to_owned(),
            reason,
        };
        let Some(remote) = &mut self.remote else {
            return Err(HostError::Unsupported(
                "links are made only where one component runs".to_owned(),
            ));
        };
        let stream = TcpStream::connect(address)
            .map_err(|error| failed(format!("at {address} cannot be opened: {error}")))?;
        let mut link = Link::new(stream).map_err(|error| failed(error.to_string()))?;
        link.write(&json!({ "hello": { "from": remote.me, "to": peer } }))
            .map_err(failed)?;
        let link = Arc::new(Mutex::new(link));
        for component in self.components.values_mut() {
            component
                .sender
                .connect(peer, LinkChannel(Arc::clone(&link)));
        }
        remote.outgoing.insert(peer.to_owned(), link);
        Ok(())
    }

    /// Takes every connection waiting on `listener`, and links each whose `hello` names this
    /// component and a component whose events lead here and that has no link yet; returns how
    /// many it linked. A connection that says anything else is refused ([`Host::refused`]); one
    /// that closes before it says `hello`, or says nothing for 5 s, is dropped.
    ///
    /// # Errors
    /// [`HostError::Channel`] when the listener fails; [`HostError::Unsupported`] on a host of
    /// every component.
    pub fn accept(&mut self, listener: &TcpListener) -> Result<usize, HostError> {
        let Some(remote) = &mut self.remote else {
            return Err(HostError::Unsupported(
                "links are accepted only where one component runs".to_owned(),
            ));
        };
        let sink = self
            .components
            .get(&remote.me)
            .map(|component| component.sink.clone())
            .unwrap_or_default();
        listener.set_nonblocking(true).map_err(ChannelError::from)?;
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Ok(link) = Link::new(stream) {
                        remote.pending.push((link, Instant::now()));
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(ChannelError::from(error).into()),
            }
        }
        let mut linked = 0;
        for (mut link, since) in std::mem::take(&mut remote.pending) {
            link.poll();
            let Some((from, to)) = link.hello.clone() else {
                if link.closed.is_none() && since.elapsed() < HELLO_WITHIN {
                    remote.pending.push((link, since));
                }
                continue;
            };
            let refusal = if to != remote.me {
                Some(format!("a link from {from} for {to}, not {}", remote.me))
            } else if !remote.expected.contains(&from) {
                Some(format!(
                    "a link from {from}, whose events do not lead to {}",
                    remote.me
                ))
            } else if remote.incoming.contains_key(&from) {
                Some(format!("a second link from {from}"))
            } else {
                None
            };
            if let Some(refusal) = refusal {
                remote.refused.push(refusal);
                let _ = link.stream.shutdown(Shutdown::Both);
                continue;
            }
            let link = Arc::new(Mutex::new(link));
            let receiver = Receiver::new(&remote.me, LinkChannel(Arc::clone(&link)))
                .with_telemetry(sink.clone());
            remote.incoming.insert(from, (link, receiver));
            linked += 1;
        }
        Ok(linked)
    }

    /// The connections [`Host::accept`] refused since the last call, each with why.
    pub fn refused(&mut self) -> Vec<String> {
        self.remote
            .as_mut()
            .map(|remote| std::mem::take(&mut remote.refused))
            .unwrap_or_default()
    }

    /// The peers with no link yet: each a component its events lead to and not connected, or
    /// one whose events lead here and that has not linked; empty once every link is open.
    pub fn unlinked(&self) -> Vec<String> {
        let Some(remote) = &self.remote else {
            return Vec::new();
        };
        let mut unlinked = self
            .peers()
            .into_keys()
            .filter(|peer| !remote.outgoing.contains_key(peer))
            .map(|peer| format!("{peer} (to)"))
            .collect::<Vec<_>>();
        unlinked.extend(
            remote
                .expected
                .iter()
                .filter(|peer| !remote.incoming.contains_key(*peer))
                .map(|peer| format!("{peer} (from)")),
        );
        unlinked
    }

    /// Whether a peer has ended the run.
    pub fn ended(&self) -> bool {
        self.remote.as_ref().is_some_and(|remote| remote.ended)
    }

    /// Ends the run: says `end` on every link, then waits at most `within` for each peer to
    /// close it. A link already gone is passed over.
    pub fn end(&mut self, within: Duration) {
        let Some(remote) = &mut self.remote else {
            return;
        };
        remote.ended = true;
        let links = remote
            .outgoing
            .values()
            .chain(remote.incoming.values().map(|(link, _)| link))
            .cloned()
            .collect::<Vec<_>>();
        for link in &links {
            let mut link = held(link);
            if link.write(&json!({ "end": true })).is_ok() {
                let _ = link.stream.shutdown(Shutdown::Write);
            }
        }
        let deadline = Instant::now() + within;
        while Instant::now() < deadline
            && links.iter().any(|link| {
                let mut link = held(link);
                link.poll();
                link.closed.is_none()
            })
        {
            std::thread::sleep(SETTLE_POLL);
        }
    }

    /// Runs every envelope that has arrived, and settles each as the module documentation says;
    /// returns without waiting.
    ///
    /// # Errors
    /// [`HostError::Peer`] naming a peer whose link closed or failed before the run ended, and
    /// as [`Host::advance`].
    pub fn deliver(&mut self) -> Result<Report, HostError> {
        let mut report = Report::default();
        if self.remote.is_some() {
            self.settle_remote(&mut report, &mut Deliveries::default(), false)?;
        } else {
            self.settle(&mut report, &mut Deliveries::default())?;
        }
        Ok(report)
    }

    /// The counters of `component`, shared, for a telemetry sender; `None` when no such
    /// component runs here.
    pub fn shared_counters(&self, component: &str) -> Option<Arc<Counters>> {
        self.components
            .get(component)
            .map(|component| Arc::clone(&component.counters))
    }

    /// [`Host::settle`] where one component runs: runs what arrives and, when `waiting`, returns
    /// only once every envelope sent here has settled (see the module documentation).
    pub(super) fn settle_remote(
        &mut self,
        report: &mut Report,
        deliveries: &mut Deliveries,
        waiting: bool,
    ) -> Result<(), HostError> {
        let started = Instant::now();
        loop {
            let remote = self.remote.as_mut().ok_or_else(|| {
                HostError::Unsupported("no links where every component runs".to_owned())
            })?;
            remote.read()?;
            if remote.ended {
                return Ok(());
            }
            let mut delivered = false;
            let peers = remote.incoming.keys().cloned().collect::<Vec<_>>();
            for peer in peers {
                loop {
                    let remote = self.remote.as_mut().ok_or_else(|| {
                        HostError::Unsupported("no links where every component runs".to_owned())
                    })?;
                    let Some((_, receiver)) = remote.incoming.get_mut(&peer) else {
                        break;
                    };
                    let envelope = match receiver.receive(Duration::ZERO) {
                        Ok(Some(envelope)) => envelope,
                        Ok(None) => break,
                        Err(error) => {
                            return Err(HostError::Peer {
                                peer,
                                reason: error.to_string(),
                            });
                        }
                    };
                    if deliveries.count >= DELIVERY_LIMIT {
                        return Err(HostError::Runaway {
                            deliveries: deliveries.count,
                            bindings: std::mem::take(&mut deliveries.bindings),
                        });
                    }
                    deliveries.count += 1;
                    deliveries.bindings.insert(envelope.binding.clone());
                    delivered = true;
                    self.run(
                        &envelope.to,
                        &envelope.command.name,
                        &envelope.command.fields,
                        Cause::Delivered {
                            binding: envelope.binding.clone(),
                        },
                        report,
                    )?;
                    let remote = self.remote.as_mut().ok_or_else(|| {
                        HostError::Unsupported("no links where every component runs".to_owned())
                    })?;
                    if let Some((_, receiver)) = remote.incoming.get_mut(&peer) {
                        receiver.acknowledge(&envelope)?;
                    }
                    if waiting || remote.engaged.is_some() {
                        remote.settle(&peer)?;
                    } else {
                        remote.engaged = Some(peer.clone());
                    }
                }
            }
            for component in self.components.values_mut() {
                if component.sender.unacknowledged() > 0 {
                    component.sender.collect_acknowledgements(Duration::ZERO)?;
                }
            }
            let remote = self.remote.as_mut().ok_or_else(|| {
                HostError::Unsupported("no links where every component runs".to_owned())
            })?;
            if remote.deficit() == 0 {
                if let Some(peer) = remote.engaged.take() {
                    remote.settle(&peer)?;
                }
                if waiting {
                    return Ok(());
                }
            }
            if !waiting {
                if delivered {
                    continue;
                }
                return Ok(());
            }
            if remote.stop.as_ref().is_some_and(|stop| stop()) {
                return Err(HostError::Stopped);
            }
            if started.elapsed() >= SETTLE_WITHIN {
                let unsettled = remote
                    .outgoing
                    .iter()
                    .filter(|(_, link)| {
                        let link = held(link);
                        link.sent > link.settled
                    })
                    .map(|(peer, _)| peer.clone())
                    .collect::<Vec<_>>();
                return Err(HostError::Peer {
                    peer: unsettled.join(", "),
                    reason: format!("settled nothing within {SETTLE_WITHIN:?}"),
                });
            }
            if !delivered {
                std::thread::sleep(SETTLE_POLL);
            }
        }
    }
}
