//! The HTTP server: one socket on 127.0.0.1, axum on a thread of its own, and every request for
//! a route handed to the caller's thread as an [`Exchange`].

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{ALLOW, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value};
use tokio::sync::oneshot;

use crate::answer::Answer;
use crate::surface::{Resolved, Route, Surface};

/// The largest request body read, in bytes; a larger one is refused `413`.
pub const BODY_LIMIT: usize = 1 << 20;

/// One request for a route, waiting for its answer.
#[derive(Debug)]
pub struct Exchange {
    route: Route,
    carried: Map<String, Value>,
    body: Vec<u8>,
    reply: oneshot::Sender<Answer>,
}

impl Exchange {
    /// The route the request is for.
    pub fn route(&self) -> &Route {
        &self.route
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
        let _ = self.reply.send(answer);
    }
}

struct Served {
    surface: Surface,
    exchanges: mpsc::Sender<Exchange>,
}

/// An HTTP listener on 127.0.0.1 serving one [`Surface`].
///
/// Dropping it stops the server: requests not yet answered are answered `503`.
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
    /// A request whose path no route has is refused `404`, and one for a served path under
    /// another method `405` with `Allow`; both with `{"refused": <why>}`.
    ///
    /// # Errors
    /// When the socket cannot be bound or the server thread cannot start.
    pub fn bind(port: u16, surface: Surface) -> io::Result<Self> {
        let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()?;
        let (sender, exchanges) = mpsc::channel();
        let (shutdown, stopped) = oneshot::channel::<()>();
        let served = Arc::new(Served {
            surface,
            exchanges: sender,
        });
        std::thread::Builder::new()
            .name("spindle-ingress".to_owned())
            .spawn(move || {
                runtime.block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(socket)?;
                    let app = axum::Router::new().fallback(handle).with_state(served);
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = stopped.await;
                        })
                        .await
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

async fn handle(State(served): State<Arc<Served>>, request: Request) -> Response {
    let (route, carried) = match served
        .surface
        .resolve(request.method().as_str(), request.uri().path())
    {
        Resolved::Found(route, carried) => (route.clone(), carried),
        Resolved::Malformed(why) => return written(Answer::refusal(400, why)),
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
            return response;
        }
        Resolved::NotFound => {
            return written(Answer::refusal(
                404,
                format!("no route serves {}", request.uri().path()),
            ));
        }
    };
    let body = match axum::body::to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body.to_vec(),
        Err(error) => {
            return written(Answer::refusal(
                413,
                format!("the body could not be read within {BODY_LIMIT} bytes: {error}"),
            ));
        }
    };
    let (reply, answered) = oneshot::channel();
    let exchange = Exchange {
        route,
        carried,
        body,
        reply,
    };
    if served.exchanges.send(exchange).is_err() {
        return written(Answer::refusal(503, "the listener is stopping"));
    }
    match answered.await {
        Ok(answer) => written(answer),
        Err(_) => written(Answer::refusal(
            503,
            "the listener stopped before answering",
        )),
    }
}

fn written(answer: Answer) -> Response {
    let status = StatusCode::from_u16(answer.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, Body::from(answer.body.to_string())).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}
