//! The observe API and the UI, served on `127.0.0.1` with a token and a `Host` check.
//!
//! Every request is checked in this order: its `Host` header must be `127.0.0.1:<port>` or
//! `localhost:<port>` (`403` otherwise, which stops DNS rebinding from another site); it must carry
//! the token as `?t=<token>` or `Authorization: Bearer <token>` (`401` otherwise, which stops other
//! local users); and it must be a `GET` (`405`), since the API is read only. Only then is its path
//! read. A refusal carries `{"refused": <why>}` and nothing of the deployment.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body, Bytes, Frame, Incoming};
use hyper::header::{ALLOW, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, HOST, HeaderValue};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use crate::answer::InstanceState;
use crate::hub::Hub;
use crate::ui;

/// How long a connection may take to send a request's head.
const HEADER_READ: Duration = Duration::from_secs(10);
/// How long an instance's ingress may take to answer a view.
const VIEW_ANSWER: Duration = Duration::from_secs(10);

type Answer = Response<BoxBody<Bytes, Infallible>>;

/// Where each view of a component is served on its instances' ingress.
pub(crate) type ViewPaths = BTreeMap<String, BTreeMap<String, String>>;

pub(crate) struct Served {
    pub(crate) hub: Arc<Hub>,
    pub(crate) token: String,
    /// The token every request to an instance's ingress carries.
    pub(crate) ingress_token: String,
    pub(crate) port: u16,
    /// Instance → its ingress, for each instance of a component reached over the network.
    pub(crate) ingress: BTreeMap<String, SocketAddr>,
    /// Component → view → path on the ingress.
    pub(crate) views: ViewPaths,
}

/// The listener: hyper on a thread of its own, stopped when dropped.
pub(crate) struct Server {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Binds `127.0.0.1:<port>`, `0` for one the system chooses.
    pub(crate) fn bind(port: u16) -> io::Result<(std::net::TcpListener, SocketAddr)> {
        let socket = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        Ok((socket, address))
    }

    /// Serves `served` on `socket`.
    pub(crate) fn start(socket: std::net::TcpListener, served: Served) -> io::Result<Self> {
        let address = socket.local_addr()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let (shutdown, mut stopped) = oneshot::channel::<()>();
        let served = Arc::new(served);
        let thread = std::thread::Builder::new()
            .name("spindle-control".to_owned())
            .spawn(move || {
                let _ = runtime.block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(socket)?;
                    loop {
                        tokio::select! {
                            _ = &mut stopped => return Ok::<(), io::Error>(()),
                            accepted = listener.accept() => match accepted {
                                Ok((stream, _)) => {
                                    tokio::spawn(connection(Arc::clone(&served), stream));
                                }
                                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                            },
                        }
                    }
                });
            })?;
        Ok(Self {
            address,
            shutdown: Some(shutdown),
            thread: Some(thread),
        })
    }

    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn connection(served: Arc<Served>, stream: tokio::net::TcpStream) {
    let service = hyper::service::service_fn(move |request| {
        let served = Arc::clone(&served);
        async move { Ok::<_, Infallible>(handle(&served, request).await) }
    });
    let _ = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ)
        .serve_connection(TokioIo::new(stream), service)
        .await;
}

fn full(status: StatusCode, content_type: &str, body: impl Into<Bytes>) -> Answer {
    let mut answer = Response::new(Full::new(body.into()).boxed());
    *answer.status_mut() = status;
    if let Ok(value) = HeaderValue::from_str(content_type) {
        answer.headers_mut().insert(CONTENT_TYPE, value);
    }
    answer
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    answer
}

fn json_answer(status: StatusCode, body: impl Into<Bytes>) -> Answer {
    full(status, "application/json", body)
}

fn refused(status: StatusCode, why: &str) -> Answer {
    json_answer(status, json!({ "refused": why }).to_string())
}

/// Whether the `Host` header names this listener: `127.0.0.1:<port>` or `localhost:<port>`.
fn host_is_own(request: &Request<Incoming>, port: u16) -> bool {
    let Some(host) = request
        .headers()
        .get(HOST)
        .and_then(|host| host.to_str().ok())
    else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
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

/// Whether the request carries `token`, as `?t=` or as a bearer credential.
fn carries_token(request: &Request<Incoming>, token: &str) -> bool {
    let in_query = request.uri().query().is_some_and(|query| {
        query
            .split('&')
            .filter_map(|pair| pair.strip_prefix("t="))
            .any(|given| same(given, token))
    });
    let in_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|given| same(given.trim(), token));
    in_query || in_header
}

fn query_value(request: &Request<Incoming>, name: &str) -> Option<String> {
    request.uri().query().and_then(|query| {
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| decode(value))?
        })
    })
}

/// `text` with every `%XX` decoded; `None` when an escape is malformed or the result not UTF-8.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

async fn handle(served: &Served, request: Request<Incoming>) -> Answer {
    if !host_is_own(&request, served.port) {
        return refused(
            StatusCode::FORBIDDEN,
            "the Host header names neither 127.0.0.1 nor localhost on this port",
        );
    }
    if !carries_token(&request, &served.token) {
        return refused(StatusCode::UNAUTHORIZED, "the token is missing or wrong");
    }
    if request.method() != hyper::Method::GET {
        let mut answer = refused(
            StatusCode::METHOD_NOT_ALLOWED,
            "the observe API is read only",
        );
        answer
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET"));
        return answer;
    }
    let path = request.uri().path().to_owned();
    match path.as_str() {
        "/api/deployment" => json_answer(StatusCode::OK, served.hub.deployment()),
        "/api/catalogue" => {
            let component = query_value(&request, "component").unwrap_or_default();
            match served.hub.catalogue(&component) {
                Some(catalogue) => json_answer(StatusCode::OK, catalogue.to_owned()),
                None => refused(
                    StatusCode::NOT_FOUND,
                    &format!("no component {component:?}"),
                ),
            }
        }
        "/api/stream" => stream(served),
        "/" | "/index.html" => match ui::index(&served.token) {
            Some(page) => full(StatusCode::OK, ui::content_type(ui::INDEX), page),
            None => refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "this build carries no UI: ui/dist was not built before b10x-spindle",
            ),
        },
        _ => {
            if let Some(rest) = path.strip_prefix("/api/views/") {
                return view(served, rest).await;
            }
            let name = path.trim_start_matches('/');
            match ui::file(name).filter(|_| !name.starts_with("api/")) {
                Some(bytes) => full(StatusCode::OK, ui::content_type(name), bytes),
                None => refused(StatusCode::NOT_FOUND, &format!("no route {path}")),
            }
        }
    }
}

/// `GET /api/stream`: Server-Sent Events, `sample` and `state` messages.
fn stream(served: &Served) -> Answer {
    let events = Events(served.hub.subscribe());
    let mut answer = Response::new(BoxBody::new(events));
    answer.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    answer
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    answer
}

/// A stream's body: each message as it arrives, until the control process stops.
struct Events(mpsc::Receiver<Bytes>);

impl Body for Events {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.0
            .poll_recv(context)
            .map(|message| message.map(|bytes| Ok(Frame::data(bytes))))
    }
}

/// `GET /api/views/<component>/<view>`: the view's answer, asked of the ingress of a running
/// instance of the component with a `GET`, answered with its status and body as they are.
async fn view(served: &Served, rest: &str) -> Answer {
    let Some((component, view)) = rest
        .split_once('/')
        .and_then(|(component, view)| Some((decode(component)?, decode(view)?)))
    else {
        return refused(
            StatusCode::NOT_FOUND,
            "a view is asked as /api/views/<component>/<view>",
        );
    };
    let Some(declared) = served.hub.views(&component) else {
        return refused(
            StatusCode::NOT_FOUND,
            &format!("no component {component:?}"),
        );
    };
    if !declared.contains(&view) {
        return refused(
            StatusCode::NOT_FOUND,
            &format!("component {component} declares no view {view:?}"),
        );
    }
    let Some(path) = served
        .views
        .get(&component)
        .and_then(|views| views.get(&view))
    else {
        return refused(
            StatusCode::NOT_FOUND,
            &format!(
                "no ingress serves {view}: {component} is not reached over the network, or its \
                 ingress serves the view at no plain path"
            ),
        );
    };
    let running = served.ingress.iter().find(|(instance, _)| {
        served
            .hub
            .instance(instance)
            .is_some_and(|(of, state)| of == component && state == InstanceState::Running)
    });
    let Some((instance, address)) = running else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            &format!("no instance of {component} is running"),
        );
    };
    match tokio::time::timeout(VIEW_ANSWER, ask(*address, path, &served.ingress_token)).await {
        Ok(Ok((status, body))) => json_answer(status, body),
        Ok(Err(error)) => refused(
            StatusCode::BAD_GATEWAY,
            &format!("the ingress of {instance} at {address}: {error}"),
        ),
        Err(_) => refused(
            StatusCode::GATEWAY_TIMEOUT,
            &format!(
                "the ingress of {instance} at {address} did not answer within {VIEW_ANSWER:?}"
            ),
        ),
    }
}

async fn ask(address: SocketAddr, path: &str, token: &str) -> Result<(StatusCode, Bytes), String> {
    let stream = tokio::net::TcpStream::connect(address)
        .await
        .map_err(|error| error.to_string())?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|error| error.to_string())?;
    tokio::spawn(connection);
    let request = Request::get(path)
        .header(HOST, address.to_string())
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Empty::<Bytes>::new())
        .map_err(|error| error.to_string())?;
    let answer = sender
        .send_request(request)
        .await
        .map_err(|error| error.to_string())?;
    let status = answer.status();
    let body = answer
        .into_body()
        .collect()
        .await
        .map_err(|error| error.to_string())?
        .to_bytes();
    Ok((status, body))
}
