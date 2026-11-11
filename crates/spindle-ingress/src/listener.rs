//! The HTTP server: one socket, on 127.0.0.1 unless the caller names another address
//! ([`Listener::bind_reading_on`]), hyper on a thread of its own, and every request for
//! a route handed to the caller's thread as an [`Exchange`], answered there by the router
//! ([`crate::route`]); on a listener given the host's [`Reader`], every read answered through it
//! on a blocking thread of its own instead ([`Listener::bind_reading`]).

use std::convert::Infallible;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, mpsc};
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
use tokio::sync::oneshot;

use spindle_host::{Failed, Host, Reader};

use crate::answer::Answer;
use crate::console::{Console, Worlds};
use crate::observe::{Answered, By, Note, Reserved, Target};
use crate::router::{Asked, BODY_LIMIT, Pending, Response as Reply};
use crate::surface::{Caller, Route, Surface};

/// How long a connection may take to send a request's head; a slower one is closed.
pub const HEADER_READ: Duration = Duration::from_secs(10);

/// How long a request's body may take to arrive once its head has; a slower one is refused `408`.
pub const BODY_READ: Duration = Duration::from_secs(10);

/// How long the listener waits after accepting a connection failed, such as when the process is
/// out of descriptors, before it accepts again.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

/// One request for a route, waiting for its answer.
#[derive(Debug)]
pub struct Exchange {
    pending: Pending,
    reply: oneshot::Sender<(Reply, Answered)>,
}

impl Exchange {
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
    /// ([`Listener::bind_reading`]).
    reader: Option<Reader>,
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

/// Compares two strings in time that depends only on their lengths.
fn same(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}

/// An HTTP/1.1 listener serving one [`Surface`], on 127.0.0.1 unless bound with
/// [`Listener::bind_reading_on`].
///
/// Dropping it stops accepting: requests not yet answered are answered `503`.
#[derive(Debug)]
pub struct Listener {
    address: SocketAddr,
    exchanges: Option<mpsc::Receiver<Exchange>>,
    shutdown: Option<oneshot::Sender<()>>,
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
    /// on.
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
    /// before anything else of it is read. On loopback each request is checked as on 127.0.0.1.
    ///
    /// # Errors
    /// [`io::ErrorKind::InvalidInput`], before anything is bound, for a surface without the
    /// proxy's secret off loopback; otherwise as [`Listener::bind`].
    pub fn bind_reading_on(
        ip: IpAddr,
        port: u16,
        surface: Surface,
        reader: Reader,
    ) -> io::Result<Self> {
        let surface = if exposed(ip) {
            surface.requiring_secret()
        } else {
            surface
        };
        Self::serve(SocketAddr::new(ip, port), surface, None, Some(reader))
    }

    fn serve(
        address: SocketAddr,
        surface: Surface,
        token: Option<String>,
        reader: Option<Reader>,
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
        let (shutdown, mut stopped) = oneshot::channel::<()>();
        let served = Arc::new(Served {
            surface,
            exchanges: sender,
            guard,
            reader,
        });
        std::thread::Builder::new()
            .name("spindle-ingress".to_owned())
            .spawn(move || {
                runtime.block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(socket)?;
                    loop {
                        tokio::select! {
                            _ = &mut stopped => return Ok::<(), io::Error>(()),
                            accepted = listener.accept() => match accepted {
                                Ok((stream, _)) => {
                                    tokio::spawn(connection(Arc::clone(&served), stream));
                                }
                                Err(_) => tokio::time::sleep(ACCEPT_BACKOFF).await,
                            },
                        }
                    }
                })
            })?;
        Ok(Self {
            address,
            exchanges: Some(exchanges),
            shutdown: Some(shutdown),
        })
    }

    /// The address the listener is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    /// The next request for a route, waiting at most `timeout`; `None` when none arrived.
    pub fn next(&self, timeout: Duration) -> Option<Exchange> {
        self.exchanges.as_ref()?.recv_timeout(timeout).ok()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.exchanges.take();
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

/// Serves one connection, HTTP/1.1, its heads bounded by [`HEADER_READ`].
async fn connection(served: Arc<Served>, stream: tokio::net::TcpStream) {
    let service = hyper::service::service_fn(move |request| handle(Arc::clone(&served), request));
    let _ = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ)
        .serve_connection(TokioIo::new(stream), service)
        .await;
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
    if let Some(reserved) = reserved {
        let query = request.uri().query().unwrap_or_default();
        let query = match &served.guard {
            Some(guard) => guard.strip(query),
            None => query.to_owned(),
        };
        let reply = crate::observe::reserved(
            served.surface.recorder(),
            served.surface.panel(),
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
/// [`Surface::SECRET_HEADER`], compared in time that depends only on the lengths ([`same`]);
/// `None` where either header is absent or given more than once, or the secret is another. No
/// value is printed.
fn proven(headers: &HeaderMap, header: &str, secret: &str) -> Option<Caller> {
    if !presents(headers, secret) {
        return None;
    }
    Caller::proven(once(headers, header)?.as_bytes())
}

/// Whether `headers` carry `secret` once in [`Surface::SECRET_HEADER`], compared in time that
/// depends only on the lengths ([`same`]).
fn presents(headers: &HeaderMap, secret: &str) -> bool {
    once(headers, Surface::SECRET_HEADER)
        .and_then(|presented| presented.to_str().ok())
        .is_some_and(|presented| same(presented, secret))
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
async fn handed(served: &Served, pending: Pending) -> Result<(Reply, Answered), Answer> {
    let pending = match &served.reader {
        Some(reader) if pending.reads(reader) => {
            let reader = reader.clone();
            match tokio::task::spawn_blocking(move || pending.read(&reader)).await {
                Ok(Ok(read)) => return Ok(read),
                Ok(Err(pending)) => *pending,
                Err(_) => return Err(Answer::refusal(500, "the read stopped before answering")),
            }
        }
        _ => pending,
    };
    let (reply, replying) = oneshot::channel();
    if served.exchanges.send(Exchange { pending, reply }).is_err() {
        return Err(Answer::refusal(503, "the listener is stopping"));
    }
    replying
        .await
        .map_err(|_| Answer::refusal(503, "the listener stopped before answering"))
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
    use super::Guard;

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
