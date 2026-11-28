//! The HTTP server: one socket, on 127.0.0.1 unless the caller names another address
//! ([`Listener::bind_reading_on`]), hyper on a thread of its own, and every request for
//! a route handed to the caller's thread as an [`Exchange`], answered there by the router
//! ([`crate::route`]); on a listener given the host's [`Reader`], every read answered through it
//! on a blocking thread of its own instead ([`Listener::bind_reading`]); over TLS, with or without
//! a client certificate required, where it is given a TLS configuration
//! ([`Listener::bind_reading_on_tls`]).

use std::convert::Infallible;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime};

use ess_http::Operation;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    ALLOW, AUTHORIZATION, CONNECTION, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST,
    HeaderMap, HeaderName, HeaderValue, TRANSFER_ENCODING,
};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{Map, Value};
use tokio::sync::{Semaphore, oneshot, watch};
use tokio::task::JoinSet;

use spindle_host::{Failed, Host, Reader};

use crate::answer::Answer;
use crate::console::{Console, Worlds};
use crate::health::{LIVENESS, Probed, SeedState};
use crate::observe::{Answered, By, Note, Reserved, Target};
use crate::router::{Asked, BODY_LIMIT, Pending, Response as Reply};
use crate::surface::{Caller, Limits, Route, Surface, Unverified};

/// How long a connection may take to send a request's head, the next one of a keep-alive
/// connection included; a slower one is closed. The default of [`Limits::head_read`].
pub const HEADER_READ: Duration = Limits::DEFAULT.head_read;

/// How long a request's body may take to arrive once its head has; a slower one is refused `408`.
pub const BODY_READ: Duration = Duration::from_secs(10);

/// How long the listener waits after accepting a connection failed, such as when the process is
/// out of descriptors, before it accepts again.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

/// An exchange no thread has taken yet ([`Exchange::claim`]).
const QUEUED: u8 = 0;
/// An exchange [`Listener::next`] handed to the caller's thread.
const TAKEN: u8 = 1;
/// An exchange its request time passed while queued, answered `503` and never handed.
const LAPSED: u8 = 2;

/// One request for a route, waiting for its answer.
#[derive(Debug)]
pub struct Exchange {
    pending: Pending,
    reply: oneshot::Sender<(Reply, Answered)>,
    /// Who decides the exchange first ([`QUEUED`], [`TAKEN`], [`LAPSED`]): the caller's thread
    /// taking it, or its request time passing (`story:listener-limits`, D3 A).
    claim: Arc<AtomicU8>,
}

impl Exchange {
    /// Claims the exchange for the caller's thread; `false` where its request time passed first,
    /// and it was answered `503` without it.
    fn take(&self) -> bool {
        self.claim
            .compare_exchange(QUEUED, TAKEN, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// The route the request is for; `None` for a console route, the metrics or the topology.
    pub fn route(&self) -> Option<&Route> {
        match &self.pending.asked {
            Asked::Route(route) => Some(route.as_ref()),
            Asked::Console(_) | Asked::Hosted(_) => None,
        }
    }

    /// The console route the request is for; `None` for a command, a view, the metrics or the
    /// topology.
    pub fn console(&self) -> Option<Console> {
        match self.pending.asked {
            Asked::Console(console) => Some(console),
            Asked::Route(_) | Asked::Hosted(_) => None,
        }
    }

    /// The request is for the metrics, `GET /__spindle/metrics`, which [`Exchange::serve`] reads
    /// from the host's counters and stores.
    pub fn metrics(&self) -> bool {
        matches!(self.pending.asked, Asked::Hosted(Reserved::Metrics))
    }

    /// The input the request's path carries in the route's variables. Empty where nothing is read
    /// before the grant check: for a route the check refuses ([`Route::not_granted`]), and on one
    /// whose requests prove their caller ([`Route::proves`]), whose path is read as the exchange
    /// is served, once the caller and its grant are checked.
    pub fn carried(&self) -> &Map<String, Value> {
        &self.pending.carried
    }

    /// The request's query string, after the `?` and still percent-encoded; empty without one. On
    /// a guarded listener the token's `t=<token>` is not part of it.
    pub fn query(&self) -> &str {
        &self.pending.query
    }

    /// The request's `Content-Type`, where it has one that is visible ASCII.
    pub fn content_type(&self) -> Option<&str> {
        self.pending.content_type.as_deref()
    }

    /// The request body.
    pub fn body(&self) -> &[u8] {
        &self.pending.body
    }

    /// Sends `answer` to the caller; a caller that has gone away is not an error.
    pub fn respond(self, answer: Answer) {
        let answered = Answered::of(&answer);
        let _ = self.reply.send((answer.into(), answered));
    }

    /// Answers the request on `host` and sends the answer: a command or view as
    /// [`answer`](crate::answer) answers it, a console route as [`crate::console`] does. Returns
    /// every command a binding invoked on the way that did not execute ([`crate::answered`]). A
    /// reset that carries a world is refused; [`Exchange::serve_with`] loads one.
    pub fn serve(self, host: &mut Host) -> Vec<Failed> {
        self.serve_loading(host, None)
    }

    /// [`Exchange::serve`], a reset's world read by `worlds` ([`crate::console::reset`]).
    pub fn serve_with(self, host: &mut Host, worlds: Worlds<'_>) -> Vec<Failed> {
        self.serve_loading(host, Some(worlds))
    }

    /// [`Exchange::serve_with`], its answer replaced, once the request was served, by the one
    /// `instead` gives where it gives one: a run whose store refused a write because another
    /// process took the store's lease answers `503`, not the store's refusal
    /// (`story:several-writers-one-postgres`). `instead` is told whether the answer served is a
    /// failure that committed nothing (a status of `500` or above with `committed: false`), as a
    /// write whose database session was ended.
    pub fn serve_with_unless(
        self,
        host: &mut Host,
        worlds: Worlds<'_>,
        instead: impl FnOnce(bool) -> Option<Answer>,
    ) -> Vec<Failed> {
        let (reply, failed, answered) = self.pending.serve(host, Some(worlds));
        let uncommitted = reply.status >= 500
            && serde_json::from_str::<Value>(&reply.body)
                .ok()
                .and_then(|body| body.get("committed").and_then(Value::as_bool))
                == Some(false);
        let reply = instead(uncommitted).map_or(reply, Into::into);
        let _ = self.reply.send((reply, answered));
        failed
    }

    fn serve_loading(self, host: &mut Host, worlds: Option<Worlds<'_>>) -> Vec<Failed> {
        let (reply, failed, answered) = self.pending.serve(host, worlds);
        let _ = self.reply.send((reply, answered));
        failed
    }
}

struct Served {
    surface: Surface,
    exchanges: mpsc::Sender<Exchange>,
    guard: Option<Guard>,
    /// The host's reader, where the listener answers reads beside the host's thread
    /// ([`Listener::bind_reading`]); set once a listener standing by leads ([`Listener::lead`]).
    reader: Arc<OnceLock<Reader>>,
    /// What the health routes read beside the caller's thread.
    health: Arc<Health>,
    /// The TLS each connection is served over, where the listener serves TLS
    /// ([`Listener::bind_reading_on_tls`]).
    tls: Option<tokio_rustls::TlsAcceptor>,
}

/// What `GET /__spindle/live` and `GET /__spindle/ready` read beside the caller's thread
/// (`story:health-and-graceful-drain`): when the caller last asked for a request
/// ([`Listener::next`]), whether it holds a request it took, how long it may go without asking,
/// and whether the run is stopping.
#[derive(Debug)]
struct Health {
    /// When the listener was bound; the instants below are counted from it.
    since: Instant,
    /// Milliseconds from `since` to the caller's last ask.
    asked: AtomicU64,
    /// Whether the caller holds a request [`Listener::next`] handed it: set when it hands one,
    /// cleared at the caller's next ask.
    holding: AtomicBool,
    /// The liveness bound, in milliseconds ([`Listener::live_within`]).
    bound: AtomicU64,
    /// Set once the run stops taking work ([`Listener::stopping`]).
    stopping: Arc<AtomicBool>,
    /// Set while the run stands by for another process's lease, until it leads
    /// ([`Listener::bind_standing_by_on`], [`Listener::lead`]).
    standby: AtomicBool,
}

impl Health {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            asked: AtomicU64::new(0),
            holding: AtomicBool::new(false),
            bound: AtomicU64::new(millis(LIVENESS)),
            stopping: Arc::new(AtomicBool::new(false)),
            standby: AtomicBool::new(false),
        }
    }

    fn standby(&self) -> bool {
        self.standby.load(Ordering::SeqCst)
    }

    /// The caller asks for a request now.
    fn beat(&self) {
        self.asked
            .store(millis(self.since.elapsed()), Ordering::SeqCst);
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// What the health routes read now, the run's seed being `seed`.
    fn probed<'a>(&self, seed: Option<&'a SeedState>) -> Probed<'a> {
        let asked = Duration::from_millis(self.asked.load(Ordering::SeqCst));
        Probed {
            seed,
            stopping: self.stopping(),
            standby: self.standby(),
            holding: self.holding.load(Ordering::SeqCst),
            idle: self.since.elapsed().saturating_sub(asked),
            bound: Duration::from_millis(self.bound.load(Ordering::SeqCst)),
        }
    }
}

/// `duration` in whole milliseconds.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// What every request to a guarded listener must carry: a `Host` header naming the listener on
/// the loopback address, and the token.
struct Guard {
    port: u16,
    token: String,
}

impl Guard {
    /// The refusal of `request`, if it is refused: `403` for a `Host` that is neither
    /// `127.0.0.1:<port>` nor `localhost:<port>`, `401` without the token as
    /// `Authorization: Bearer <token>` or `?t=<token>`.
    fn refuses(&self, request: &Request<Incoming>) -> Option<Answer> {
        let host = request
            .headers()
            .get(HOST)
            .and_then(|host| host.to_str().ok())
            .map(str::to_ascii_lowercase);
        let own = [
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ];
        if !host.is_some_and(|host| own.contains(&host)) {
            return Some(Answer::refusal(
                403,
                "the Host header names neither 127.0.0.1 nor localhost on this port",
            ));
        }
        let in_header = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|given| same(given.trim(), &self.token));
        let in_query = request.uri().query().is_some_and(|query| {
            query
                .split('&')
                .filter_map(|pair| pair.strip_prefix("t="))
                .any(|given| same(given, &self.token))
        });
        (!in_header && !in_query).then(|| Answer::refusal(401, "the token is missing or wrong"))
    }

    /// `query` without the pairs that carry the token, which are the listener's and no
    /// command's input.
    fn strip(&self, query: &str) -> String {
        query
            .split('&')
            .filter(|pair| {
                !pair
                    .strip_prefix("t=")
                    .is_some_and(|given| same(given, &self.token))
            })
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// Whether `ip` is off loopback: anything but `127.0.0.0/8` and `::1`, the unspecified addresses
/// and `::ffff:127.0.0.1` included. A listener bound to one requires the proxy's secret on every
/// request ([`Listener::bind_reading_on`]).
pub fn exposed(ip: IpAddr) -> bool {
    !ip.is_loopback()
}

/// `127.0.0.1:<port>`.
fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// Whether `given` is `secret`, compared in time that depends only on the secret's length: every
/// byte of `secret` is read against `given`'s byte at its place, or a zero past `given`'s end,
/// whatever `given`'s length, so the time tells a caller neither where they differ nor how long
/// the secret is (`story:listener-limits`).
fn same(given: &str, secret: &str) -> bool {
    let given = given.as_bytes();
    let differ = secret.bytes().enumerate().fold(
        u8::from(given.len() != secret.len()),
        |differ, (index, expected)| {
            #[cfg(test)]
            tests::COMPARED.with(|compared| compared.set(compared.get() + 1));
            differ | (given.get(index).copied().unwrap_or(0) ^ expected)
        },
    );
    differ == 0
}

/// An HTTP/1.1 listener serving one [`Surface`], on 127.0.0.1 unless bound with
/// [`Listener::bind_reading_on`].
///
/// Dropping it stops accepting and ends the server thread at once: an answer not yet written is
/// not written. [`Listener::close`] answers every request it holds before the thread ends.
///
/// Beside every route it answers the run's health on its own threads, with or without a recorder
/// (`story:health-and-graceful-drain`): `GET /__spindle/live` `200` while the caller holds a
/// request it took or asked for one ([`Listener::next`]) within [`LIVENESS`], or the bound given
/// ([`Listener::live_within`]), and `503` once it has done neither; `GET /__spindle/ready` `503` once the
/// run stops taking work ([`Listener::stopping`]).
#[derive(Debug)]
pub struct Listener {
    address: SocketAddr,
    exchanges: Option<mpsc::Receiver<Exchange>>,
    shutdown: Option<oneshot::Sender<Stop>>,
    server: Option<std::thread::JoinHandle<io::Result<()>>>,
    health: Arc<Health>,
    /// The host's reader the server answers reads through, once it has one ([`Listener::lead`]).
    reader: Arc<OnceLock<Reader>>,
}

/// How the server thread stops.
#[derive(Debug)]
enum Stop {
    /// At once, every connection dropped ([`Drop`]).
    Now,
    /// Once every connection has written the answer it holds and closed, waiting at most this
    /// long ([`Listener::close`]).
    Draining(Duration),
}

impl Listener {
    /// Binds `127.0.0.1:<port>`, `0` for a port the system chooses, and starts serving
    /// `surface`.
    ///
    /// Each refusal is `{"refused": <why>}`, as ESS's served application writes it: a path no
    /// route has `404`; a served path under another method `405` with `Allow`, the method
    /// compared exactly; a path variable that does not decode `400`; a body sent with
    /// `Transfer-Encoding` `411`, since only one announced by `Content-Length` is read; a body
    /// announced over [`BODY_LIMIT`] `413` before it is read; one that does not arrive within
    /// [`BODY_READ`] `408`, and one that breaks off `400`. A head not sent within
    /// [`HEADER_READ`] closes the connection. A failed accept is retried, and the listener goes
    /// on. The surface's [`Limits`] bound the connections it holds, each head and each request's
    /// time ([`Surface::limited`]).
    ///
    /// On a surface made [`Surface::proving_callers`] each request for a command or view is
    /// handed on with the caller its header proves where it also carries the proxy's secret, both
    /// read once and never printed; the router refuses one that proves none. A console route or
    /// one under `/__spindle/` answers the proxy's secret alone, and without it is refused `403`
    /// `{"refused": "not granted", "actor": null}` before anything is read.
    ///
    /// # Errors
    /// When the socket cannot be bound or the server thread cannot start.
    pub fn bind(port: u16, surface: Surface) -> io::Result<Self> {
        Self::serve(loopback(port), surface, None, None)
    }

    /// As [`Listener::bind`], every request first checked: one whose `Host` header is neither
    /// `127.0.0.1:<port>` nor `localhost:<port>` is refused `403`, and one that does not carry
    /// `token` as `Authorization: Bearer <token>` or `?t=<token>` `401`, before its route is
    /// read. What a launcher starts on a host other users share serves this way.
    ///
    /// # Errors
    /// As [`Listener::bind`].
    pub fn bind_guarded(port: u16, surface: Surface, token: &str) -> io::Result<Self> {
        Self::serve(loopback(port), surface, Some(token.to_owned()), None)
    }

    /// As [`Listener::bind`], every read answered beside the host's thread through the host's
    /// `reader` ([`spindle_host::Host::reader`]), each on a thread of its own, so a read waits
    /// neither for a command, a binding or a tick the host's thread runs nor for another read.
    ///
    /// The reads are a view whose owning component's store the reader reads ([`Reader::reads`]),
    /// `GET /api/model`, `GET /api/events` and, on an observing surface, `GET /__spindle/metrics`
    /// and `GET /__spindle/topology`; a view answers the rows the last write that landed left,
    /// never part of a write. The caller is proven and the grants checked as on the host's
    /// thread. Every command, `POST /api/reset` and a view whose store is read on the host's
    /// thread only still arrive on [`Listener::next`], one at a time in arrival order.
    ///
    /// # Errors
    /// As [`Listener::bind`].
    pub fn bind_reading(port: u16, surface: Surface, reader: Reader) -> io::Result<Self> {
        Self::bind_reading_on(IpAddr::V4(Ipv4Addr::LOCALHOST), port, surface, reader)
    }

    /// As [`Listener::bind_reading`], bound to `ip`, IPv4 or IPv6, instead of 127.0.0.1. Off
    /// loopback ([`exposed`]) the surface must be made [`Surface::proving_callers`], and is served
    /// [`Surface::requiring_secret`]: every request without the proxy's secret is refused `403`
    /// before anything else of it is read. A surface made [`Surface::verifying_tokens`] is served
    /// off loopback only over TLS ([`Listener::bind_reading_on_tls`]). On loopback each request is
    /// checked as on 127.0.0.1.
    ///
    /// # Errors
    /// [`io::ErrorKind::InvalidInput`], before anything is bound, for a surface without the
    /// proxy's secret off loopback, or one verifying tokens; otherwise as [`Listener::bind`].
    pub fn bind_reading_on(
        ip: IpAddr,
        port: u16,
        surface: Surface,
        reader: Reader,
    ) -> io::Result<Self> {
        if exposed(ip) && surface.verifies_tokens() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{ip} is not loopback, and a surface verifying bearer tokens is served there \
                     only over TLS, so no token crosses the network in the clear"
                ),
            ));
        }
        let surface = if exposed(ip) {
            surface.requiring_secret()
        } else {
            surface
        };
        Self::serve(SocketAddr::new(ip, port), surface, None, Some(reader))
    }

    /// As [`Listener::bind_reading_on`], every connection served over TLS as `tls` says
    /// (`story:listener-tls`): its certificate and key, and, where it verifies clients, the
    /// client certificate it requires. Each connection's handshake runs on that connection's own
    /// task, bounded by [`HEADER_READ`] and given up once the listener closes, so a client that
    /// stalls it holds neither the accept loop nor the drain; one that fails it, a plain HTTP
    /// request included, is closed with no HTTP read or written. Every route is behind it, the
    /// run's own under `/__spindle/` included. Off loopback a surface made
    /// [`Surface::verifying_tokens`] is served as it is: its token takes the place of the proxy's
    /// secret, every request without a valid one refused `401` before anything else is read.
    ///
    /// # Errors
    /// As [`Listener::bind_reading_on`], but for a surface verifying tokens off loopback.
    pub fn bind_reading_on_tls(
        ip: IpAddr,
        port: u16,
        surface: Surface,
        reader: Reader,
        tls: Arc<rustls::ServerConfig>,
    ) -> io::Result<Self> {
        let surface = if exposed(ip) && !surface.verifies_tokens() {
            surface.requiring_secret()
        } else {
            surface
        };
        Self::serve_over(
            SocketAddr::new(ip, port),
            surface,
            None,
            Some(reader),
            Some(tokio_rustls::TlsAcceptor::from(tls)),
        )
    }

    /// As [`Listener::bind_reading_on`], over TLS where `tls` is given
    /// ([`Listener::bind_reading_on_tls`]), bound before the host exists, for a run standing by
    /// for another process's lease (`story:several-writers-one-postgres`): `GET /__spindle/live`
    /// answers as on any listener, `GET /__spindle/ready` `503` with `standby: true`, and every
    /// other request `503` at once, none handed to [`Listener::next`], until the run leads
    /// ([`Listener::lead`]).
    ///
    /// # Errors
    /// As [`Listener::bind_reading_on`].
    pub fn bind_standing_by_on(
        ip: IpAddr,
        port: u16,
        surface: Surface,
        tls: Option<Arc<rustls::ServerConfig>>,
    ) -> io::Result<Self> {
        let surface = if exposed(ip) {
            surface.requiring_secret()
        } else {
            surface
        };
        let listener = Self::serve_over(
            SocketAddr::new(ip, port),
            surface,
            None,
            None,
            tls.map(tokio_rustls::TlsAcceptor::from),
        )?;
        listener.health.standby.store(true, Ordering::SeqCst);
        Ok(listener)
    }

    /// A listener standing by ([`Listener::bind_standing_by_on`]) leads: from now on it answers
    /// every read through the host's `reader`, hands every other request to [`Listener::next`],
    /// and reads ready as any listener does. A listener that has its reader keeps it.
    pub fn lead(&self, reader: Reader) {
        let _ = self
            .reader
            .set(reader.stopped_by(Arc::clone(&self.health.stopping)));
        self.health.standby.store(false, Ordering::SeqCst);
    }

    fn serve(
        address: SocketAddr,
        surface: Surface,
        token: Option<String>,
        reader: Option<Reader>,
    ) -> io::Result<Self> {
        Self::serve_over(address, surface, token, reader, None)
    }

    /// [`Listener::serve`], each connection over TLS where `tls` is given.
    fn serve_over(
        address: SocketAddr,
        surface: Surface,
        token: Option<String>,
        reader: Option<Reader>,
        tls: Option<tokio_rustls::TlsAcceptor>,
    ) -> io::Result<Self> {
        if surface.secret_required() && surface.caller_secret().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{address} requires the proxy's secret on every request, and the surface has \
                     none: serve it proving callers"
                ),
            ));
        }
        let socket = std::net::TcpListener::bind(address)?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        let guard = token.map(|token| Guard {
            port: address.port(),
            token,
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let (sender, exchanges) = mpsc::channel();
        let (shutdown, mut stopped) = oneshot::channel::<Stop>();
        let health = Arc::new(Health::new());
        // One place per connection held: at the cap the loop accepts no connection until one
        // closes, and the kernel's backlog holds the rest (`story:listener-limits`, D2 A).
        let places = Arc::new(Semaphore::new(
            surface
                .limits()
                .connections
                .clamp(1, Semaphore::MAX_PERMITS),
        ));
        // Its reads of the events are cut off where they wait once the run stops taking work, so a
        // read waiting for a store's feed does not hold the listener's close
        // (`story:leader-exits-promptly-on-sigterm`).
        let reader = Arc::new(
            reader
                .map(|reader| OnceLock::from(reader.stopped_by(Arc::clone(&health.stopping))))
                .unwrap_or_default(),
        );
        let served = Arc::new(Served {
            surface,
            exchanges: sender,
            guard,
            reader: Arc::clone(&reader),
            health: Arc::clone(&health),
            tls,
        });
        let server = std::thread::Builder::new()
            .name("spindle-ingress".to_owned())
            .spawn(move || {
                runtime.block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(socket)?;
                    let (closing, closed) = watch::channel(false);
                    let mut connections = JoinSet::new();
                    let grace = loop {
                        tokio::select! {
                            stop = &mut stopped => match stop {
                                Ok(Stop::Draining(grace)) => break grace,
                                Ok(Stop::Now) | Err(_) => return Ok::<(), io::Error>(()),
                            },
                            accepted = async {
                                let place = Arc::clone(&places).acquire_owned().await;
                                (place, listener.accept().await)
                            } => match accepted {
                                (Ok(place), Ok((stream, _))) => {
                                    let (served, closed) = (Arc::clone(&served), closed.clone());
                                    connections.spawn(async move {
                                        connection(served, stream, closed).await;
                                        drop(place);
                                    });
                                }
                                // The places are never closed.
                                (Err(_), Ok(_)) => {}
                                (_, Err(_)) => tokio::time::sleep(ACCEPT_BACKOFF).await,
                            },
                            Some(_) = connections.join_next(), if !connections.is_empty() => {}
                        }
                    };
                    // No connection is accepted from here on; each one open writes the answer it
                    // holds, then closes.
                    drop(listener);
                    let _ = closing.send(true);
                    let _ = tokio::time::timeout(grace, async {
                        while connections.join_next().await.is_some() {}
                    })
                    .await;
                    Ok(())
                })
            })?;
        Ok(Self {
            address,
            exchanges: Some(exchanges),
            shutdown: Some(shutdown),
            server: Some(server),
            health,
            reader,
        })
    }

    /// The address the listener is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    /// The next request for a route, waiting at most `timeout`; `None` when none arrived. Each
    /// call counts as the caller's thread answering for `GET /__spindle/live`, as it starts and as
    /// it returns; from a request it hands until the next call, the caller holds that request and
    /// reads live however long it takes to answer it. A request whose request time passed while
    /// it was queued was answered `503` and is never handed ([`Surface::limited`]).
    pub fn next(&self, timeout: Duration) -> Option<Exchange> {
        self.health.holding.store(false, Ordering::SeqCst);
        self.health.beat();
        let exchanges = self.exchanges.as_ref()?;
        let until = Instant::now().checked_add(timeout);
        let next = loop {
            let left = until.map_or(timeout, |until| {
                until.saturating_duration_since(Instant::now())
            });
            match exchanges.recv_timeout(left) {
                Ok(exchange) if exchange.take() => break Some(exchange),
                Ok(_) => {}
                Err(_) => break None,
            }
        };
        self.health.beat();
        self.health.holding.store(next.is_some(), Ordering::SeqCst);
        next
    }

    /// This listener answering `GET /__spindle/live` `503` once the caller, holding no request,
    /// has not asked for one ([`Listener::next`]) for longer than `bound`, in place of
    /// [`LIVENESS`]. A served run keeps [`LIVENESS`].
    #[must_use]
    pub fn live_within(self, bound: Duration) -> Self {
        self.health.bound.store(millis(bound), Ordering::SeqCst);
        self
    }

    /// The flag that stops this listener taking work (`story:health-and-graceful-drain`): once it
    /// is set, `GET /__spindle/ready` answers `503`, and every request that arrives from then on is
    /// answered `503` at once, but those under `/__spindle/`; each request already handed over
    /// still arrives on [`Listener::next`]. A read of the events or of a history under way beside
    /// the caller's thread when it is set is cut off and answered `503` where it waits for the
    /// store, for its feed or its turn ([`Reader::stopped_by`]), so no such wait holds
    /// [`Listener::close`]; a read that completes is answered normally.
    /// It is set by whoever holds it, such as a signal handler, without the caller's thread
    /// (`signal_hook::flag::register`).
    pub fn stopping(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.health.stopping)
    }

    /// Stops the listener so that no client reads a closed connection where an answer was due:
    /// each request handed over and not yet taken with [`Listener::next`] is answered `503`, and
    /// so is each one that reaches the listener from now on; no connection is accepted; every
    /// answer already given to an [`Exchange`] is written, and each connection closed once its
    /// answer is. Returns once the server thread has ended, waiting at most `grace` for the
    /// connections; one still open then, such as a client still sending its body, is dropped.
    pub fn close(mut self, grace: Duration) {
        if let Some(exchanges) = self.exchanges.take() {
            while let Ok(exchange) = exchanges.try_recv() {
                exchange.respond(Answer::refusal(503, "the listener is closing"));
            }
        }
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(Stop::Draining(grace));
        }
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.exchanges.take();
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(Stop::Now);
        }
    }
}

/// Serves one connection, HTTP/1.1, each head bounded by the surface's [`Limits::head_read`]
/// ([`HEADER_READ`] by default), [`Limits::head_bytes`] and [`Limits::headers`]; over TLS where
/// the listener serves it, once the handshake has completed within [`Limits::head_read`]. A
/// handshake that fails or does not complete in time, or is under way once `closing` reads `true`,
/// closes the connection with no HTTP read or written. Once `closing` reads `true`, the connection
/// writes the answer it holds, if any, and closes.
async fn connection(
    served: Arc<Served>,
    stream: tokio::net::TcpStream,
    mut closing: watch::Receiver<bool>,
) {
    let Some(tls) = served.tls.clone() else {
        return answering(served, stream, closing).await;
    };
    let handshake = tokio::time::timeout(served.surface.limits().head_read, tls.accept(stream));
    let stream = tokio::select! {
        shaken = handshake => match shaken {
            Ok(Ok(stream)) => stream,
            Ok(Err(_)) | Err(_) => return,
        },
        _ = closing.wait_for(|closing| *closing) => return,
    };
    answering(served, stream, closing).await;
}

/// Serves HTTP/1.1 on `stream`, as [`connection`] says.
async fn answering<S>(served: Arc<Served>, stream: S, mut closing: watch::Receiver<bool>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let limits = served.surface.limits();
    let service = hyper::service::service_fn(move |request| handle(Arc::clone(&served), request));
    // hyper answers a head over either bound `431` and closes, before the service runs; its head
    // read time also runs while a kept-alive connection waits for its next head, so an idle one
    // holds a place of the cap for at most that long.
    let serving = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(limits.head_read)
        .max_header_size(limits.head_bytes)
        .max_headers(limits.headers)
        .serve_connection(TokioIo::new(stream), service);
    tokio::pin!(serving);
    tokio::select! {
        _ = serving.as_mut() => {}
        () = async {
            let _ = closing.wait_for(|closing| *closing).await;
        } => {
            serving.as_mut().graceful_shutdown();
            let _ = serving.await;
        }
    }
}

async fn handle(
    served: Arc<Served>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let Some(recorder) = served.surface.recorder().cloned() else {
        return Ok(replied(answer(&served, request).await.0));
    };
    let at = SystemTime::now();
    let started = Instant::now();
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    // The query string as the routes read it: without the token a guarded listener reads.
    let query = request.uri().query().unwrap_or_default();
    let query = match &served.guard {
        Some(guard) => guard.strip(query),
        None => query.to_owned(),
    };
    let (reply, note) = answer(&served, request).await;
    if let Some(note) = note {
        let target = Target {
            path: &path,
            query: &query,
        };
        recorder.record(at, started.elapsed(), &method, target, &reply, note);
    }
    Ok(replied(reply))
}

/// The answer to `request`, and what a record notes of it; no note for a path under the reserved
/// prefix, which is neither recorded nor counted.
async fn answer(served: &Served, request: Request<Incoming>) -> (Reply, Option<Note>) {
    let reserved = Reserved::of(request.uri().path());
    let unrouted = |by| reserved.is_none().then(|| Note::unrouted(by));
    if let Some(refusal) = served
        .guard
        .as_ref()
        .and_then(|guard| guard.refuses(&request))
    {
        return (refusal.into(), unrouted(By::Ingress));
    }
    // On a surface verifying bearer tokens, nothing of a request is read until its token
    // verifies, but at the probes: not its route, its method, its body or its claims. A refusal
    // is neither recorded nor counted, as the proxy's secret refusal is not
    // (`story:verified-caller-token`, D7).
    let verified = match served.surface.caller_token() {
        Some(tokens) if !crate::surface::probe(reserved) => {
            match bearer(request.headers()).map(|token| tokens.verifier().caller(token)) {
                Some(Ok(caller)) => Some(caller),
                Some(Err(Unverified)) | None => return (Answer::unauthenticated().into(), None),
            }
        }
        _ => None,
    };
    // The run's own routes but the probes are operator tooling: a token that verifies and maps to
    // no caller reads none of them, refused as a request authenticated as no caller and, as every
    // answer under the prefix, neither recorded nor counted (`story:verified-caller-token`, F2).
    let unmapped = verified.as_ref().is_some_and(Option::is_none);
    if unmapped && reserved.is_some() {
        return (Answer::not_proven().into(), None);
    }
    // On a surface whose requests prove their caller, the run's own routes are operator tooling
    // behind the proxy: they answer the proxy's secret alone.
    let unproxied = served
        .surface
        .caller_secret()
        .is_some_and(|secret| !presents(request.headers(), secret.expose()));
    // Off loopback, or wherever the surface requires it, nothing of a request without the secret
    // is read: not its route, its method, its body or its caller. It is neither recorded nor
    // counted, so a caller on the network cannot evict the operator's records.
    if unproxied && served.surface.secret_required() {
        return (Answer::not_proven().into(), None);
    }
    if reserved.is_some() && unproxied {
        return (Answer::not_proven().into(), None);
    }
    // Once the run stops taking work, a request arriving is answered at once and never queued
    // (`story:health-and-graceful-drain`); the run's own routes still answer, readiness `503`.
    if reserved.is_none() && served.health.stopping() {
        return (stopping().into(), None);
    }
    // A run standing by for another process's lease answers every request but its own routes
    // `503` at once (`story:several-writers-one-postgres`).
    if reserved.is_none() && served.health.standby() {
        return (standing_by().into(), None);
    }
    if let Some(reserved) = reserved {
        let query = request.uri().query().unwrap_or_default();
        let query = match &served.guard {
            Some(guard) => guard.strip(query),
            None => query.to_owned(),
        };
        let reply = crate::observe::reserved(
            served.surface.recorder(),
            served.surface.panel(),
            &served.health.probed(served.surface.seed()),
            reserved,
            request.method().as_str(),
            request.uri().path(),
            &query,
        );
        if let Some(reply) = reply {
            return (reply, None);
        }
        // The metrics and the topology read the counters and kept rows: answered beside the host's
        // thread where the listener has its reader, else on it.
        let pending = Pending {
            asked: Asked::Hosted(reserved),
            carried: Map::new(),
            path: request.uri().path().to_owned(),
            query,
            content_type: None,
            body: Vec::new(),
            recorder: served.surface.recorder().cloned(),
            served: None,
            proven: None,
        };
        let reply = match handed(served, pending).await {
            Ok((reply, _)) => reply,
            Err(refusal) => refusal.into(),
        };
        return (reply, None);
    }
    let (asked, carried) = match crate::router::ask(
        &served.surface,
        request.method().as_str(),
        request.uri().path(),
    ) {
        Ok(asked) => asked,
        Err((refusal, by, Some(asked))) => {
            let answered = Answered {
                by,
                ..Answered::ingress()
            };
            return (refusal, Some(noted(&asked, answered)));
        }
        Err((refusal, by, None)) => return (refusal, unrouted(by)),
    };
    let refused = |answer: Answer| -> (Reply, Option<Note>) {
        (answer.into(), Some(noted(&asked, Answered::ingress())))
    };
    if matches!(asked, Asked::Console(_)) && unproxied {
        return refused(Answer::not_proven());
    }
    if request.headers().contains_key(TRANSFER_ENCODING) {
        return refused(Answer::refusal(
            411,
            "this surface reads a body announced by `Content-Length`; chunked transfer is not read",
        ));
    }
    let announced = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if announced.is_some_and(|length| length > BODY_LIMIT as u64) {
        return refused(crate::router::oversized());
    }
    let query = request.uri().query().unwrap_or_default();
    let query = match &served.guard {
        Some(guard) => guard.strip(query),
        None => query.to_owned(),
    };
    let content_type = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let proven = match (
        &asked,
        served.surface.caller_header(),
        served.surface.caller_secret(),
    ) {
        (Asked::Route(_), _, _) if verified.is_some() => verified.flatten(),
        (Asked::Route(_), Some(header), Some(secret)) => {
            proven(request.headers(), header, secret.expose())
        }
        _ => None,
    };
    let path = request.uri().path().to_owned();
    let read = tokio::time::timeout(
        BODY_READ,
        Limited::new(request.into_body(), BODY_LIMIT).collect(),
    )
    .await;
    let body = match read {
        Ok(Ok(collected)) => collected.to_bytes().to_vec(),
        Ok(Err(error)) => {
            return refused(Answer::refusal(
                400,
                format!("the body could not be read: {error}"),
            ));
        }
        Err(_) => {
            return refused(Answer::refusal(
                408,
                format!("the body did not arrive within {BODY_READ:?}"),
            ));
        }
    };
    let unanswered = noted(&asked, Answered::ingress());
    let pending = Pending {
        asked,
        carried,
        path,
        query,
        content_type,
        body,
        recorder: served.surface.recorder().cloned(),
        served: served.surface.served(),
        proven,
    };
    match handed(served, pending).await {
        Ok((reply, answered)) => (
            reply,
            Some(Note {
                answered,
                ..unanswered
            }),
        ),
        Err(refusal) => (refusal.into(), Some(unanswered)),
    }
}

/// The caller `headers` prove in `header` ([`Caller::proven`]), where they also carry `secret` in
/// [`Surface::SECRET_HEADER`], compared in time that depends only on the secret's length
/// ([`same`]); `None` where either header is absent or given more than once, or the secret is
/// another. No value is printed.
fn proven(headers: &HeaderMap, header: &str, secret: &str) -> Option<Caller> {
    if !presents(headers, secret) {
        return None;
    }
    Caller::proven(once(headers, header)?.as_bytes())
}

/// Whether `headers` carry `secret` once in [`Surface::SECRET_HEADER`], compared in time that
/// depends only on the secret's length ([`same`]).
fn presents(headers: &HeaderMap, secret: &str) -> bool {
    once(headers, Surface::SECRET_HEADER)
        .and_then(|presented| presented.to_str().ok())
        .is_some_and(|presented| same(presented, secret))
}

/// The bearer token `headers` carry once in `Authorization` (`Bearer <token>`, the scheme in any
/// case); `None` where the header is absent, given more than once, of another scheme, or empty.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = once(headers, AUTHORIZATION.as_str())?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim_start_matches(' ');
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

/// The one value of the header `name` in `headers`; `None` where it is absent or given more than
/// once.
fn once<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a HeaderValue> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

/// The answer to `pending`: a read the listener's reader answers ([`Pending::reads`]) on a
/// blocking thread of its own, beside the host's thread and every other read; anything else
/// handed to the host's thread as an [`Exchange`]. The `503` refusal where the listener is
/// stopping or stopped before answering, and `500` where a read stopped without an answer.
///
/// Answered within the surface's [`Limits::request_time`] of now, the end of the request's body
/// read (`story:listener-limits`, D3 A, D4 A): a request no thread has taken by then is answered
/// `503` with `committed: false` and [`Listener::next`] never hands it; one taken, or a read under
/// way, `504`, the thread's later answer dropped.
async fn handed(served: &Served, pending: Pending) -> Result<(Reply, Answered), Answer> {
    let time = served.surface.limits().request_time;
    // A time past what the clock can name sets no deadline.
    let deadline = tokio::time::Instant::now().checked_add(time);
    let pending = match served.reader.get() {
        Some(reader) if pending.reads(reader) => {
            let reader = reader.clone();
            let read = tokio::task::spawn_blocking(move || pending.read(&reader));
            match within(deadline, read).await {
                Some(Ok(Ok(read))) => return Ok(read),
                Some(Ok(Err(pending))) => *pending,
                Some(Err(_)) => {
                    return Err(Answer::refusal(500, "the read stopped before answering"));
                }
                None => return Err(late(time)),
            }
        }
        _ => pending,
    };
    // A request whose body was still arriving when the run stopped taking work is not queued.
    if served.health.stopping() && !matches!(pending.asked, Asked::Hosted(_)) {
        return Err(stopping());
    }
    // A standby has no host to answer the metrics or the topology.
    if served.health.standby() {
        return Err(standing_by());
    }
    let (reply, replying) = oneshot::channel();
    let claim = Arc::new(AtomicU8::new(QUEUED));
    let exchange = Exchange {
        pending,
        reply,
        claim: Arc::clone(&claim),
    };
    if served.exchanges.send(exchange).is_err() {
        return Err(Answer::refusal(503, "the listener is stopping"));
    }
    match within(deadline, replying).await {
        Some(answered) => {
            answered.map_err(|_| Answer::refusal(503, "the listener stopped before answering"))
        }
        None if claim
            .compare_exchange(QUEUED, LAPSED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok() =>
        {
            Err(unclaimed(time))
        }
        None => Err(late(time)),
    }
}

/// `future`'s output, or `None` once `deadline` has passed first; no deadline waits for it.
async fn within<F: Future>(deadline: Option<tokio::time::Instant>, future: F) -> Option<F::Output> {
    match deadline {
        Some(deadline) => tokio::time::timeout_at(deadline, future).await.ok(),
        None => Some(future.await),
    }
}

/// The answer to a request no thread took within its request `time`, which is never handed:
/// `503` with `committed: false`.
fn unclaimed(time: Duration) -> Answer {
    let mut answer = Answer::refusal(
        503,
        format!("no thread took the request within its request time of {time:?}"),
    );
    if let Some(Value::Object(body)) = &mut answer.body {
        body.insert("committed".to_owned(), Value::Bool(false));
    }
    answer
}

/// The answer to a request taken and not answered within its request `time`, whose command
/// still completes.
fn late(time: Duration) -> Answer {
    Answer::refusal(
        504,
        format!("the request was not answered within its request time of {time:?}"),
    )
}

/// The refusal of a request that arrives once the run stops taking work.
fn stopping() -> Answer {
    Answer::refusal(503, "the run is stopping")
}

/// The refusal of a request to a run standing by for another process's lease.
fn standing_by() -> Answer {
    Answer::refusal(
        503,
        "the run is on standby: another process holds the lease of its store",
    )
}

/// What a record notes of a request for `asked`, answered as `answered` says: the route
/// template, and the qualified command or view, or the console route as `GET /api/model`; none
/// of them for the metrics or the topology.
fn noted(asked: &Asked, answered: Answered) -> Note {
    match asked {
        Asked::Route(route) => Note {
            route: Some(route.path.clone()),
            operation: Some(route.operation.name().to_string()),
            kind: Some(match route.operation {
                Operation::Command(_) => "command",
                Operation::View(_) => "view",
            }),
            answered,
        },
        Asked::Console(console) => Note {
            route: Some(console.path().to_owned()),
            operation: Some(format!(
                "{} {}",
                console.method().as_str().to_uppercase(),
                console.path()
            )),
            kind: Some("console"),
            answered,
        },
        // Under the reserved prefix: the listener notes no request there, so no record holds it.
        Asked::Hosted(_) => Note {
            route: None,
            operation: None,
            kind: None,
            answered,
        },
    }
}

fn replied(reply: Reply) -> Response<Full<Bytes>> {
    let bodied = !reply.body.is_empty();
    let mut response = Response::new(Full::new(Bytes::from(reply.body)));
    *response.status_mut() =
        StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if reply.panel {
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static(crate::observe::PANEL_MEDIA),
        );
        response.headers_mut().insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(crate::observe::PANEL_POLICY),
        );
    } else if reply.metrics {
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static(crate::observe::metrics::METRICS_MEDIA),
        );
    } else if bodied {
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static(crate::router::CONTENT_TYPE),
        );
    }
    if let Some(value) = reply
        .allow
        .as_deref()
        .and_then(|allow| HeaderValue::from_str(allow).ok())
    {
        response.headers_mut().insert(ALLOW, value);
    }
    // The headers the binding declares, after those this listener writes, none of which one of
    // them replaces, and never the framing hyper writes: ESS refuses a binding that declares
    // `Content-Type`, `Content-Length`, `Transfer-Encoding` or `Connection` (`ESS-HTTP-026`), so
    // this only holds the line. Each name is a token and each value field-value text by now
    // (`answer::declared_headers`).
    for (name, value) in &reply.headers {
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) else {
            continue;
        };
        if response.headers().contains_key(&name)
            || [CONTENT_LENGTH, TRANSFER_ENCODING, CONNECTION].contains(&name)
        {
            continue;
        }
        response.headers_mut().append(name, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{Guard, same};

    thread_local! {
        /// How many byte pairs [`same`] compared on this thread.
        pub(super) static COMPARED: Cell<usize> = const { Cell::new(0) };
    }

    /// How many byte pairs `same(given, secret)` compares, and its answer.
    fn compared(given: &str, secret: &str) -> (usize, bool) {
        COMPARED.with(|compared| compared.set(0));
        let answer = same(given, secret);
        (COMPARED.with(Cell::get), answer)
    }

    #[test]
    fn the_secret_compare_takes_the_same_time_for_any_length() {
        let secret = "s3cret-of-the-proxy";
        let (equal, matched) = compared(secret, secret);
        assert!(matched, "the secret matches itself");
        assert_eq!(equal, secret.len(), "the whole secret is read");
        for given in [
            "",
            "s",
            "s3cret-of-the-prox",
            "s3cret-of-the-proxZ",
            "s3cret-of-the-proxy!",
            "s3cret-of-the-proxy-and-much-more-than-it",
        ] {
            let (read, matched) = compared(given, secret);
            assert!(!matched, "{given:?} is not the secret");
            assert_eq!(
                read, equal,
                "{given:?} reads as many bytes as the secret's own length"
            );
        }
    }

    #[test]
    fn the_token_pair_is_not_query_input() {
        let guard = Guard {
            port: 1,
            token: "secret".to_owned(),
        };
        assert_eq!(guard.strip("t=secret&reason=worn"), "reason=worn");
        assert_eq!(guard.strip("reason=worn&t=secret"), "reason=worn");
        assert_eq!(guard.strip("t=secret"), "");
        assert_eq!(
            guard.strip("t=other&reason=worn"),
            "t=other&reason=worn",
            "a `t` that is not the token is the request's own"
        );
    }
}
