//! Adversarial cases for `story:http-router-is-transport-free`: the router's own refusals (the
//! body limit, its order against the route checks) and the fixed caller's grant, each compared
//! with what the listener writes for the same request where the listener can be asked.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::Map;
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{BODY_LIMIT, Caller, HTTP_LOOPBACK, Listener, Request, Response, Surface};

const STEPS: usize = 50;
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir(name: &str) -> EssIr {
    spindle_spec::load(&fixtures().join(name))
        .unwrap_or_else(|error| panic!("the {name} fixture compiles: {error:?}"))
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-router\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn host(ir: EssIr, state: &Path) -> Host {
    Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state)
        .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"))
}

fn desk_surface(ir: &EssIr) -> Surface {
    Surface::new(ir, &["desk".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"))
}

fn path(surface: &Surface, operation: &str) -> String {
    surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("the surface serves {operation}"))
        .path
        .clone()
}

fn request(method: &str, path: &str, body: Vec<u8>) -> Request {
    Request {
        method: method.to_owned(),
        path: path.to_owned(),
        query: String::new(),
        content_type: Some("application/json".to_owned()),
        body,
    }
}

/// A request head announcing `length` body bytes and sending none: the listener refuses an
/// announced oversized body before reading it.
fn announce(address: SocketAddr, method: &str, target: &str, length: usize) -> Response {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: {length}\r\nConnection: close\r\n\r\n"
    )
    .expect("the head is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    let allow = head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case("allow")
            .then(|| value.trim().to_owned())
    });
    let panel = head.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(key, value)| {
            key.eq_ignore_ascii_case("content-type") && value.trim() == spindle_ingress::PANEL_MEDIA
        })
    });
    Response {
        status,
        allow,
        panel,
        metrics: head.lines().skip(1).any(|line| {
            line.split_once(':').is_some_and(|(key, value)| {
                key.eq_ignore_ascii_case("content-type")
                    && value.trim() == spindle_ingress::METRICS_MEDIA
            })
        }),
        headers: Vec::new(),
        body: body.to_owned(),
    }
}

fn over_http(
    listener: &Listener,
    served: &mut Host,
    method: &str,
    target: &str,
    length: usize,
) -> Response {
    let address = listener.local_addr();
    let client = {
        let (method, target) = (method.to_owned(), target.to_owned());
        thread::spawn(move || announce(address, &method, &target, length))
    };
    for _ in 0..STEPS {
        if client.is_finished() {
            break;
        }
        if let Some(exchange) = listener.next(WAIT) {
            exchange.serve(served);
        }
    }
    assert!(client.is_finished(), "{method} {target} was not answered");
    client.join().expect("the client thread")
}

/// The acceptance: the router answers the same status and body the listener writes. A body over
/// [`BODY_LIMIT`] is the one refusal both write themselves.
#[test]
fn an_oversized_body_is_refused_with_the_listeners_status_and_body() {
    let ir = ir("desk");
    let listened_state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let routed_state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut listened = host(ir.clone(), listened_state.path());
    let mut routed = host(ir.clone(), routed_state.path());
    let router = desk_surface(&ir);
    let listener = Listener::bind(0, desk_surface(&ir)).expect("the listener binds");
    let open = path(&router, "study.desk.OpenDesk");

    let written = over_http(&listener, &mut listened, "POST", &open, BODY_LIMIT + 1);
    let answered = spindle_ingress::route(
        &router,
        &mut routed,
        request("POST", &open, vec![b' '; BODY_LIMIT + 1]),
    );
    assert_eq!(written.status, 413, "the listener: {written:?}");
    assert_eq!(
        answered.response, written,
        "the router answers an oversized body as the listener writes it"
    );
}

/// The limit is inclusive: a body of exactly [`BODY_LIMIT`] bytes is read, as the listener's
/// `Limited` reads it.
#[test]
fn a_body_of_exactly_the_limit_is_read() {
    let ir = ir("desk");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut routed = host(ir.clone(), state.path());
    let router = desk_surface(&ir);
    let open = path(&router, "study.desk.OpenDesk");
    let mut body = br#"{"desk_id": "d1", "holder": "ada"}"#.to_vec();
    body.resize(BODY_LIMIT, b' ');
    let answered = spindle_ingress::route(&router, &mut routed, request("POST", &open, body));
    assert_eq!(
        answered.response.status, 202,
        "a body of exactly the limit is read: {:?}",
        answered.response
    );
}

/// The listener resolves the route before it looks at the body, so an oversized body to a path
/// no route has is `404` and to another method `405` with `Allow`; the router keeps that order.
#[test]
fn the_route_is_checked_before_the_body_limit() {
    let ir = ir("desk");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut routed = host(ir.clone(), state.path());
    let router = desk_surface(&ir);
    let open = path(&router, "study.desk.OpenDesk");
    let missing = spindle_ingress::route(
        &router,
        &mut routed,
        request("POST", "/no/such/route", vec![b' '; BODY_LIMIT + 1]),
    );
    assert_eq!(missing.response.status, 404, "{:?}", missing.response);
    let wrong = spindle_ingress::route(
        &router,
        &mut routed,
        request("GET", &open, vec![b' '; BODY_LIMIT + 1]),
    );
    assert_eq!(wrong.response.status, 405, "{:?}", wrong.response);
    assert_eq!(wrong.response.allow.as_deref(), Some("POST"));
    let reset = spindle_ingress::route(
        &router,
        &mut routed,
        request("GET", "/api/reset", Vec::new()),
    );
    assert_eq!(reset.response.status, 405, "{:?}", reset.response);
    assert_eq!(reset.response.allow.as_deref(), Some("POST"));
}

/// The fixed caller's grant runs on the router path as it does over HTTP: a read-granted view a
/// fixed caller may not read is `403` "not granted".
#[test]
fn the_router_refuses_a_view_the_fixed_caller_may_not_read() {
    let ir = ir("adversary-view-grants");
    let caller = Caller {
        actor: "desk.leaf.Guest".to_owned(),
        attributes: Map::new(),
    };
    let surface = Surface::fixing_caller(&ir, &["binder".to_owned()], None, &caller)
        .unwrap_or_else(|refused| panic!("the binder surface is served: {refused}"));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut routed = host(ir, state.path());
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "desk.leaf.LeavesOfPages")
        .expect("the view is served")
        .clone();
    let answered = spindle_ingress::route(
        &surface,
        &mut routed,
        Request {
            method: view.method.as_str().to_uppercase(),
            path: view.path.clone(),
            query: "pages=1".to_owned(),
            content_type: None,
            body: Vec::new(),
        },
    );
    assert_eq!(answered.response.status, 403, "{:?}", answered.response);
    let body: serde_json::Value =
        serde_json::from_str(&answered.response.body).expect("a JSON body");
    assert_eq!(body["refused"], "not granted", "{body}");
    assert_eq!(body["actor"], "desk.leaf.Guest", "{body}");
}
