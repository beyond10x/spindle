//! One component of a system on its own, its bindings carried over TCP links to the processes that
//! run the others.
//!
//! # Links
//!
//! A link is one TCP connection from the component that publishes to the component whose command a
//! binding invokes. Each message on it is a 4-byte big-endian length and that many bytes of JSON,
//! an object with exactly one of these keys:
//!
//! | message | sent by | means |
//! |---|---|---|
//! | `{"hello": {"from", "to"}}` | the connecting end, first and once | the components the link joins |
//! | `{"frame": <Frame>}` | the connecting end: an envelope; the accepting end: an acknowledgement | a channel frame |
//! | `{"settled": 1}` | the accepting end | the oldest envelope not yet settled on this link has run, and everything it led to |
//! | `{"end": true}` | the component named by [`Host::accept_end_from`], on either side | the run is over; the link closes next |
//!
//! Each end reads its links as a strict state machine. On an accepted connection only `hello` is
//! legal first; any other message, a message that is not JSON, a length over the limit or a close
//! in the middle of a message refuses the connection by name ([`Host::refused`]). A connection that
//! closes before it says anything changes nothing. After that, a second `hello`, a frame or a
//! `settled` on the side that does not send it, a `settled` with nothing owed, an `end` from any
//! other component, anything after `end`, or a close without `end`, is [`HostError::Peer`] naming
//! the peer. Nothing is dropped or queued silently.
//!
//! # Settling
//!
//! A call that runs a command here ([`Host::submit`], each tick of [`Host::advance`]) returns once
//! everything it led to has run on every component, as in one process. Each envelope is settled
//! once every envelope its own command sent has settled, and the envelopes of one link are settled
//! in the order they arrived, so the sender reads the `n`-th `settled` as the `n`-th envelope's.
//! This is the termination detection of Dijkstra and Scholten with an engagement per envelope: an
//! envelope that arrives while a call waits is settled on its own consequences, never on the
//! call's, and a cycle of components never waits on itself. So a tick never starts while the last
//! one's consequences are still running elsewhere.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::{Admission, Environment};
use spindle_channel::{Channel, ChannelError, Frame, Receiver, Router};
use spindle_telemetry::Counters;
use spindle_timer::{Clock, Instant};

use super::{DELIVERY_LIMIT, Deliveries, Host, HostError, Opening, Report, Retries, Stores};
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

/// Which end of the connection this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// This end connected: it sends envelopes and reads acknowledgements and `settled`.
    Opened,
    /// This end accepted: it reads `hello` and envelopes and sends acknowledgements and `settled`.
    Accepted,
}

/// One end of a link.
struct Link {
    side: Side,
    stream: TcpStream,
    buffer: Vec<u8>,
    frames: VecDeque<Frame>,
    /// The peer: given when this end connected, read from `hello` when it accepted.
    peer: Option<String>,
    /// The `to` of the `hello` an accepted connection said.
    to: Option<String>,
    /// The one component whose `end` is honoured.
    end_from: Option<String>,
    sent: u64,
    settled: u64,
    ended: bool,
    closed: Option<String>,
    violated: bool,
}

impl Link {
    fn new(
        stream: TcpStream,
        side: Side,
        peer: Option<String>,
        end_from: Option<String>,
    ) -> std::io::Result<Self> {
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        Ok(Self {
            side,
            stream,
            buffer: Vec::new(),
            frames: VecDeque::new(),
            peer,
            to: None,
            end_from,
            sent: 0,
            settled: 0,
            ended: false,
            closed: None,
            violated: false,
        })
    }

    fn violate(&mut self, reason: String) {
        if self.closed.is_none() {
            self.closed = Some(reason);
            self.violated = true;
        }
        self.buffer.clear();
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    fn write(&mut self, message: &Value) -> Result<(), String> {
        if let Some(reason) = self.broken() {
            return Err(reason.to_owned());
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
            if self.closed.is_none() {
                self.closed = Some(reason.clone());
            }
            return Err(reason);
        }
        Ok(())
    }

    /// Reads what has arrived and takes every whole message from it, in order.
    fn poll(&mut self) {
        let mut chunk = [0_u8; 8192];
        let mut eof = false;
        while self.closed.is_none() && !eof {
            match self.stream.read(&mut chunk) {
                Ok(0) => eof = true,
                Ok(read) => self.buffer.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => self.closed = Some(format!("failed a read: {error}")),
            }
        }
        while self.closed.is_none() {
            let Some(prefix) = self.buffer.first_chunk::<4>() else {
                break;
            };
            let size = u32::from_be_bytes(*prefix) as usize;
            if size > MAX_MESSAGE {
                self.violate(format!(
                    "sent a length of {size} bytes, over the limit of {MAX_MESSAGE}"
                ));
                return;
            }
            if self.buffer.len() < 4 + size {
                break;
            }
            let message = serde_json::from_slice::<Value>(&self.buffer[4..4 + size]);
            self.buffer.drain(..4 + size);
            match message {
                Ok(message) => {
                    if let Err(reason) = self.take(message) {
                        self.violate(reason);
                    }
                }
                Err(error) => self.violate(format!("sent a message that is not JSON: {error}")),
            }
        }
        if eof && self.closed.is_none() {
            if self.buffer.is_empty() {
                self.closed = Some("closed".to_owned());
            } else {
                self.violate("closed in the middle of a message".to_owned());
            }
        }
    }

    /// Takes one message, or names why it is a violation here.
    fn take(&mut self, message: Value) -> Result<(), String> {
        let Value::Object(message) = message else {
            return Err(format!("sent {message}, which is no link message"));
        };
        let mut entries = message.into_iter();
        let (Some((kind, body)), None) = (entries.next(), entries.next()) else {
            return Err(
                "sent an object without exactly one key, which is no link message".to_owned(),
            );
        };
        if self.ended {
            return Err(format!("sent {kind} after end"));
        }
        if self.side == Side::Accepted && self.peer.is_none() && kind != "hello" {
            return Err(format!("sent {kind} before hello"));
        }
        match kind.as_str() {
            "hello" => {
                if self.side == Side::Opened || self.peer.is_some() {
                    return Err("said hello a second time".to_owned());
                }
                let field = |name: &str| {
                    body.get(name)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(|| format!("said a hello without {name}"))
                };
                let (from, to) = (field("from")?, field("to")?);
                self.peer = Some(from);
                self.to = Some(to);
            }
            "frame" => {
                let frame = serde_json::from_value::<Frame>(body)
                    .map_err(|error| format!("sent a frame that does not decode: {error}"))?;
                match (self.side, &frame) {
                    (Side::Accepted, Frame::Deliver(_)) | (Side::Opened, Frame::Acknowledge(_)) => {
                        self.frames.push_back(frame);
                    }
                    (Side::Accepted, Frame::Acknowledge(_)) => {
                        return Err(
                            "sent an acknowledgement down a link only envelopes travel".to_owned()
                        );
                    }
                    (Side::Opened, Frame::Deliver(_)) => {
                        return Err("sent an envelope back up the link this end opened, where \
                                    only acknowledgements and settled travel"
                            .to_owned());
                    }
                }
            }
            "settled" => {
                if self.side == Side::Accepted {
                    return Err("sent settled down a link only envelopes travel".to_owned());
                }
                if self.settled >= self.sent {
                    return Err(format!(
                        "settled {} envelopes, and only {} were sent to it",
                        self.settled + 1,
                        self.sent
                    ));
                }
                self.settled += 1;
            }
            "end" => {
                if self.end_from.is_none() || self.end_from != self.peer {
                    return Err(match &self.end_from {
                        Some(ender) => format!("said end, which only {ender} may say"),
                        None => "said end, which no peer may say here".to_owned(),
                    });
                }
                self.ended = true;
            }
            other => return Err(format!("sent {other}, which is no link message")),
        }
        Ok(())
    }

    /// Why the link can no longer be used: a violation, or a close without `end`.
    fn broken(&self) -> Option<&str> {
        match (&self.closed, self.ended && !self.violated) {
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
    /// Reads the link first, so a message that arrived before this frame is judged against what
    /// had been sent until now.
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        let mut link = held(&self.0);
        link.poll();
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

/// An envelope this component has run and not yet settled: it is settled once every envelope its
/// command sent has, each named by its link and its place in that link's order.
struct Owed {
    sent: Vec<(String, u64)>,
}

/// What a one-component host keeps about its links.
pub(super) struct Remote {
    me: String,
    /// The components whose events lead here: one incoming link from each.
    expected: BTreeSet<String>,
    end_from: Option<String>,
    outgoing: BTreeMap<String, Shared>,
    incoming: BTreeMap<String, (Shared, Receiver)>,
    /// The envelopes run and not yet settled, by the link they came on, in arrival order.
    owed: BTreeMap<String, VecDeque<Owed>>,
    pending: Vec<(Link, Instant)>,
    refused: Vec<String>,
    ended: bool,
    stop: Option<Box<dyn Fn() -> bool + Send>>,
}

impl Remote {
    /// How many envelopes have been sent on each outgoing link.
    fn sent(&self) -> BTreeMap<String, u64> {
        self.outgoing
            .iter()
            .map(|(peer, link)| (peer.clone(), held(link).sent))
            .collect()
    }

    /// Whether every envelope up to each named place has settled.
    fn has_settled(&self, through: &[(String, u64)]) -> bool {
        through.iter().all(|(peer, place)| {
            self.outgoing
                .get(peer)
                .is_some_and(|link| held(link).settled >= *place)
        })
    }

    /// Settles, link by link and in arrival order, every owed envelope whose sends have settled.
    fn settle_ready(&mut self) -> Result<(), HostError> {
        let peers = self.owed.keys().cloned().collect::<Vec<_>>();
        for peer in peers {
            loop {
                let ready = self
                    .owed
                    .get(&peer)
                    .and_then(VecDeque::front)
                    .is_some_and(|owed| self.has_settled(&owed.sent));
                if !ready {
                    break;
                }
                if let Some(queue) = self.owed.get_mut(&peer) {
                    queue.pop_front();
                }
                let Some((link, _)) = self.incoming.get(&peer) else {
                    continue;
                };
                held(link)
                    .write(&json!({ "settled": 1 }))
                    .map_err(|reason| HostError::Peer {
                        peer: peer.clone(),
                        reason,
                    })?;
            }
        }
        Ok(())
    }

    /// Reads every link; a link that broke is an error naming its peer.
    fn read(&mut self) -> Result<(), HostError> {
        let links = self
            .outgoing
            .iter()
            .chain(self.incoming.iter().map(|(peer, (link, _))| (peer, link)));
        for (peer, link) in links {
            let mut link = held(link);
            link.poll();
            if let Some(reason) = link.broken() {
                let reason = if link.violated {
                    reason.to_owned()
                } else {
                    format!("{reason} before the run ended")
                };
                return Err(HostError::Peer {
                    peer: peer.clone(),
                    reason,
                });
            }
            self.ended |= link.ended;
        }
        Ok(())
    }

    fn missing() -> HostError {
        HostError::Unsupported("no links where every component runs".to_owned())
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
        let mut host = Self::assemble(
            ir,
            clock,
            state,
            placements,
            TCP,
            Opening {
                decider: None,
                retries: Retries::Retried,
                stores: Stores::new(),
            },
        )?;
        let expected = Router::from_shared(&host.ir)?
            .routes()
            .iter()
            .filter(|route| route.to == component && route.from != component)
            .map(|route| route.from.clone())
            .collect();
        host.remote = Some(Remote {
            me: component.to_owned(),
            expected,
            end_from: None,
            outgoing: BTreeMap::new(),
            incoming: BTreeMap::new(),
            owed: BTreeMap::new(),
            pending: Vec::new(),
            refused: Vec::new(),
            ended: false,
            stop: None,
        });
        Ok(host)
    }

    /// Every component an event binding of a component running here leads to, with those
    /// bindings, excluding the components that run here.
    pub fn peers(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut peers = BTreeMap::<String, BTreeSet<String>>::new();
        let Ok(router) = Router::from_shared(&self.ir) else {
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

    /// Honours `end` from `peer` only, on every link with it; from any other peer, and from
    /// every peer until this is called, `end` is a violation.
    pub fn accept_end_from(&mut self, peer: &str) {
        let Some(remote) = &mut self.remote else {
            return;
        };
        remote.end_from = Some(peer.to_owned());
        let links = remote
            .outgoing
            .values()
            .chain(remote.incoming.values().map(|(link, _)| link));
        for link in links {
            held(link).end_from = Some(peer.to_owned());
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
        let remote = self.remote.as_mut().ok_or_else(Remote::missing)?;
        let stream = TcpStream::connect(address)
            .map_err(|error| failed(format!("at {address} cannot be opened: {error}")))?;
        let mut link = Link::new(
            stream,
            Side::Opened,
            Some(peer.to_owned()),
            remote.end_from.clone(),
        )
        .map_err(|error| failed(error.to_string()))?;
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
    /// many it linked. A connection that breaks the link protocol before it is linked, or whose
    /// `hello` names anything else, is refused ([`Host::refused`]); one that closes before it
    /// says anything, or says nothing for 5 s, is dropped.
    ///
    /// # Errors
    /// [`HostError::Channel`] when the listener fails; [`HostError::Unsupported`] on a host of
    /// every component.
    pub fn accept(&mut self, listener: &TcpListener) -> Result<usize, HostError> {
        let remote = self.remote.as_mut().ok_or_else(Remote::missing)?;
        let sink = self
            .components
            .get(&remote.me)
            .map(|component| component.sink.clone())
            .unwrap_or_default();
        listener.set_nonblocking(true).map_err(ChannelError::from)?;
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Ok(link) =
                        Link::new(stream, Side::Accepted, None, remote.end_from.clone())
                    {
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
            let address = link
                .stream
                .peer_addr()
                .map_or_else(|_| "a peer".to_owned(), |address| address.to_string());
            if link.violated {
                let reason = link.closed.clone().unwrap_or_default();
                remote
                    .refused
                    .push(format!("a connection from {address}: {reason}"));
                continue;
            }
            let (Some(from), Some(to)) = (link.peer.clone(), link.to.clone()) else {
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
                link.broken()
                    .map(|reason| format!("a link from {from}: {reason} before it was linked"))
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

    /// Whether the peer [`Host::accept_end_from`] names has ended the run.
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
    /// [`HostError::Peer`] naming a peer that broke the link protocol or whose link closed
    /// before the run ended, and as [`Host::advance`].
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
    /// only once every envelope sent before the call has settled (see the module documentation).
    pub(super) fn settle_remote(
        &mut self,
        report: &mut Report,
        deliveries: &mut Deliveries,
        waiting: bool,
    ) -> Result<(), HostError> {
        let started = Instant::now();
        let target = self
            .remote
            .as_ref()
            .ok_or_else(Remote::missing)?
            .sent()
            .into_iter()
            .collect::<Vec<_>>();
        loop {
            let remote = self.remote.as_mut().ok_or_else(Remote::missing)?;
            remote.read()?;
            if remote.ended {
                return Ok(());
            }
            let mut delivered = false;
            let peers = remote.incoming.keys().cloned().collect::<Vec<_>>();
            for peer in peers {
                loop {
                    let remote = self.remote.as_mut().ok_or_else(Remote::missing)?;
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
                    let before = remote.sent();
                    self.invoke(&envelope, report)?;
                    let remote = self.remote.as_mut().ok_or_else(Remote::missing)?;
                    if let Some((_, receiver)) = remote.incoming.get_mut(&peer) {
                        receiver.acknowledge(&envelope)?;
                    }
                    let sent = remote
                        .sent()
                        .into_iter()
                        .filter(|(link, count)| before.get(link).is_none_or(|was| count > was))
                        .collect();
                    remote
                        .owed
                        .entry(peer.clone())
                        .or_default()
                        .push_back(Owed { sent });
                    remote.settle_ready()?;
                }
            }
            for component in self.components.values_mut() {
                if component.sender.unacknowledged() > 0 {
                    component.sender.collect_acknowledgements(Duration::ZERO)?;
                }
            }
            let remote = self.remote.as_mut().ok_or_else(Remote::missing)?;
            remote.settle_ready()?;
            if !waiting {
                if delivered {
                    continue;
                }
                return Ok(());
            }
            if remote.has_settled(&target) {
                return Ok(());
            }
            if remote.stop.as_ref().is_some_and(|stop| stop()) {
                return Err(HostError::Stopped);
            }
            if started.elapsed() >= SETTLE_WITHIN {
                let unsettled = target
                    .iter()
                    .filter(|(peer, place)| {
                        remote
                            .outgoing
                            .get(peer)
                            .is_some_and(|link| held(link).settled < *place)
                    })
                    .map(|(peer, _)| peer.clone())
                    .collect::<Vec<_>>();
                return Err(HostError::Peer {
                    peer: unsettled.join(", "),
                    reason: format!("settled nothing it owes within {SETTLE_WITHIN:?}"),
                });
            }
            if !delivered {
                std::thread::sleep(SETTLE_POLL);
            }
        }
    }
}
