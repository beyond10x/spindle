//! The HTTP server: one socket on 127.0.0.1, hyper on a thread of its own, and every request for
//! a route handed to the caller's thread as an [`Exchange`], answered there by the router
//! ([`crate::route`]).

use std::convert::Infallible;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    ALLOW, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderValue, TRANSFER_ENCODING,
};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{Map, Value};
use tokio::sync::oneshot;

use spindle_host::{Failed, Host};

use crate::answer::Answer;
use crate::console::{Console, Worlds};
use crate::router::{Asked, BODY_LIMIT, Pending, Response as Reply};
use crate::surface::{Route, Surface};

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
    reply: oneshot::Sender<Reply>,
}

impl Exchange {
    /// The route the request is for; `None` for a console route.
    pub fn route(&self) -> Option<&Route> {
        match &self.pending.asked {
            Asked::Route(route) => Some(route.as_ref()),
            Asked::Console(_) => None,
        }
    }

    /// The console route the request is for; `None` for a command or view.
    pub fn console(&self) -> Option<Console> {
        match self.pending.asked {
            Asked::Console(console) => Some(console),
            Asked::Route(_) => None,
        }
    }

    /// The input the request's path carries in the route's variables and, for a view, the
    /// parameters its query string carries ([`Route::parameters`]).
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
        let _ = self.reply.send(answer.into());
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
        let (reply, failed) = self.pending.serve(host, worlds);
        let _ = self.reply.send(reply);
        failed
    }
}

struct Served {
    surface: Surface,
    exchanges: mpsc::Sender<Exchange>,
    guard: Option<Guard>,
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

/// Compares two strings in time that depends only on their lengths.
fn same(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}

/// An HTTP/1.1 listener on 127.0.0.1 serving one [`Surface`].
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
    /// # Errors
    /// When the socket cannot be bound or the server thread cannot start.
    pub fn bind(port: u16, surface: Surface) -> io::Result<Self> {
        Self::serve(port, surface, None)
    }

    /// As [`Listener::bind`], every request first checked: one whose `Host` header is neither
    /// `127.0.0.1:<port>` nor `localhost:<port>` is refused `403`, and one that does not carry
    /// `token` as `Authorization: Bearer <token>` or `?t=<token>` `401`, before its route is
    /// read. What a launcher starts on a host other users share serves this way.
    ///
    /// # Errors
    /// As [`Listener::bind`].
    pub fn bind_guarded(port: u16, surface: Surface, token: &str) -> io::Result<Self> {
        Self::serve(port, surface, Some(token.to_owned()))
    }

    fn serve(port: u16, surface: Surface, token: Option<String>) -> io::Result<Self> {
        let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
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
    if let Some(refusal) = served
        .guard
        .as_ref()
        .and_then(|guard| guard.refuses(&request))
    {
        return Ok(written(refusal));
    }
    let (asked, carried) = match crate::router::ask(
        &served.surface,
        request.method().as_str(),
        request.uri().path(),
    ) {
        Ok(asked) => asked,
        Err(refusal) => return Ok(replied(refusal)),
    };
    if request.headers().contains_key(TRANSFER_ENCODING) {
        return Ok(written(Answer::refusal(
            411,
            "this surface reads a body announced by `Content-Length`; chunked transfer is not read",
        )));
    }
    let announced = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if announced.is_some_and(|length| length > BODY_LIMIT as u64) {
        return Ok(written(crate::router::oversized()));
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
    let read = tokio::time::timeout(
        BODY_READ,
        Limited::new(request.into_body(), BODY_LIMIT).collect(),
    )
    .await;
    let body = match read {
        Ok(Ok(collected)) => collected.to_bytes().to_vec(),
        Ok(Err(error)) => {
            return Ok(written(Answer::refusal(
                400,
                format!("the body could not be read: {error}"),
            )));
        }
        Err(_) => {
            return Ok(written(Answer::refusal(
                408,
                format!("the body did not arrive within {BODY_READ:?}"),
            )));
        }
    };
    let (reply, answered) = oneshot::channel();
    let exchange = Exchange {
        pending: Pending {
            asked,
            carried,
            query,
            content_type,
            body,
        },
        reply,
    };
    if served.exchanges.send(exchange).is_err() {
        return Ok(written(Answer::refusal(503, "the listener is stopping")));
    }
    Ok(match answered.await {
        Ok(reply) => replied(reply),
        Err(_) => written(Answer::refusal(
            503,
            "the listener stopped before answering",
        )),
    })
}

fn written(answer: Answer) -> Response<Full<Bytes>> {
    replied(answer.into())
}

fn replied(reply: Reply) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(reply.body)));
    *response.status_mut() =
        StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(crate::router::CONTENT_TYPE),
    );
    if let Some(value) = reply
        .allow
        .as_deref()
        .and_then(|allow| HeaderValue::from_str(allow).ok())
    {
        response.headers_mut().insert(ALLOW, value);
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
