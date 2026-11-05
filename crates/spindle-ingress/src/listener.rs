//! The HTTP server: one socket on 127.0.0.1, hyper on a thread of its own, and every request for
//! a route handed to the caller's thread as an [`Exchange`].

use std::convert::Infallible;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{ALLOW, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue, TRANSFER_ENCODING};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{Map, Value};
use tokio::sync::oneshot;

use spindle_host::{Failed, Host};

use crate::answer::Answer;
use crate::console::Console;
use crate::surface::{Resolved, Route, Surface};

/// The largest request body read, in bytes; one announced larger is refused `413` before it is
/// read.
pub const BODY_LIMIT: usize = 1 << 20;

/// How long a connection may take to send a request's head; a slower one is closed.
pub const HEADER_READ: Duration = Duration::from_secs(10);

/// How long a request's body may take to arrive once its head has; a slower one is refused `408`.
pub const BODY_READ: Duration = Duration::from_secs(10);

/// How long the listener waits after accepting a connection failed, such as when the process is
/// out of descriptors, before it accepts again.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

/// What a request asks for.
#[derive(Debug)]
enum Asked {
    /// A command or view the surface serves.
    Route(Box<Route>),
    /// A console route.
    Console(Console),
}

/// A status and the body written under it, as `application/json`.
#[derive(Debug)]
struct Reply {
    status: u16,
    text: String,
}

impl From<Answer> for Reply {
    fn from(answer: Answer) -> Self {
        Self {
            status: answer.status,
            text: answer.body.to_string(),
        }
    }
}

/// One request for a route, waiting for its answer.
#[derive(Debug)]
pub struct Exchange {
    asked: Asked,
    carried: Map<String, Value>,
    body: Vec<u8>,
    reply: oneshot::Sender<Reply>,
}

impl Exchange {
    /// The route the request is for; `None` for a console route.
    pub fn route(&self) -> Option<&Route> {
        match &self.asked {
            Asked::Route(route) => Some(route.as_ref()),
            Asked::Console(_) => None,
        }
    }

    /// The console route the request is for; `None` for a command or view.
    pub fn console(&self) -> Option<Console> {
        match self.asked {
            Asked::Console(console) => Some(console),
            Asked::Route(_) => None,
        }
    }

    /// The input the request's path carries in the route's variables.
    pub fn carried(&self) -> &Map<String, Value> {
        &self.carried
    }

    /// The request body.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Sends `answer` to the caller; a caller that has gone away is not an error.
    pub fn respond(self, answer: Answer) {
        let _ = self.reply.send(answer.into());
    }

    /// Answers the request on `host` and sends the answer: a command or view as
    /// [`answer`](crate::answer) answers it, a console route as [`crate::console`] does. Returns
    /// every command a binding invoked on the way that did not execute ([`crate::answered`]).
    pub fn serve(self, host: &mut Host) -> Vec<Failed> {
        let mut failed = Vec::new();
        let reply = match &self.asked {
            Asked::Route(route) => {
                let (answer, dropped) = crate::answered(host, route, &self.carried, &self.body);
                failed = dropped;
                answer.into()
            }
            Asked::Console(Console::Model) => Reply {
                status: 200,
                text: crate::console::model(host.ir()),
            },
            Asked::Console(Console::Events) => crate::console::events(host).into(),
            Asked::Console(Console::Reset) => crate::console::reset(host).into(),
        };
        let _ = self.reply.send(reply);
        failed
    }
}

struct Served {
    surface: Surface,
    exchanges: mpsc::Sender<Exchange>,
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
        let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let (sender, exchanges) = mpsc::channel();
        let (shutdown, mut stopped) = oneshot::channel::<()>();
        let served = Arc::new(Served {
            surface,
            exchanges: sender,
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
    let (asked, carried) = match served
        .surface
        .resolve(request.method().as_str(), request.uri().path())
    {
        Resolved::Found(route, carried) => (Asked::Route(Box::new(route.clone())), carried),
        Resolved::Console(console) => (Asked::Console(console), Map::new()),
        Resolved::Malformed(why) => return Ok(written(Answer::refusal(400, why))),
        Resolved::WrongMethod(allowed) => {
            let allowed = allowed
                .iter()
                .map(|method| method.as_str().to_uppercase())
                .collect::<Vec<_>>()
                .join(", ");
            let mut response =
                written(Answer::refusal(405, format!("this path answers {allowed}")));
            if let Ok(value) = HeaderValue::from_str(&allowed) {
                response.headers_mut().insert(ALLOW, value);
            }
            return Ok(response);
        }
        Resolved::NotFound => {
            return Ok(written(Answer::refusal(
                404,
                format!("no route serves {}", request.uri().path()),
            )));
        }
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
        return Ok(written(Answer::refusal(
            413,
            format!("the body is announced over {BODY_LIMIT} bytes"),
        )));
    }
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
        asked,
        carried,
        body,
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
    let mut response = Response::new(Full::new(Bytes::from(reply.text)));
    *response.status_mut() =
        StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}
