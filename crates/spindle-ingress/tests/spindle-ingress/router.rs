//! The router answers a request value on a host with no listener, socket or runtime in it
//! (`story:http-router-is-transport-free`), and its answer is the one the listener writes for the
//! same request: the same status, the same `Allow` and the same body, byte for byte.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, reached over the network;
//! its surface is served with the console routes. Two hosts run the same requests in the same
//! order, one through the listener and one through [`spindle_ingress::route`].

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{HTTP_LOOPBACK, Listener, Request, Response, Surface};

const COMPONENT: &str = "desk";

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"router\"\n",
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

fn surface(ir: &EssIr) -> Surface {
    Surface::new(ir, &[COMPONENT.to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"))
}

fn host(ir: EssIr, state: &Path) -> Host {
    Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state)
        .unwrap_or_else(|error| panic!("the host admits the desk: {error}"))
}

/// The path the surface serves `operation` at.
fn path(surface: &Surface, operation: &str) -> String {
    surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("the surface serves {operation}"))
        .path
        .clone()
}

/// One HTTP/1.1 request, `Connection: close`: the status, the `Allow` header and the body text.
fn send(
    address: SocketAddr,
    method: &str,
    target: &str,
    content_type: Option<&str>,
    body: &str,
) -> Response {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    let content_type = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\n{content_type}\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
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
    let header = |name: &str| {
        head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    };
    assert_eq!(
        header("content-type").as_deref(),
        Some(spindle_ingress::CONTENT_TYPE),
        "the listener writes every answer as {}",
        spindle_ingress::CONTENT_TYPE
    );
    Response {
        status,
        allow: header("allow"),
        panel: false,
        metrics: false,
        body: body.to_owned(),
    }
}

/// The requests both hosts answer, in order: a method, a path and query, a content type and a
/// body.
fn requests(surface: &Surface) -> Vec<(String, String, Option<String>, String)> {
    let open = path(surface, "study.desk.OpenDesk");
    let take = path(surface, "study.desk.TakeDesk");
    let book = path(surface, "study.desk.BookDesk");
    let view = path(surface, "study.desk.AllDesks");
    let json = Some("application/json".to_owned());
    vec![
        (
            "POST".into(),
            open.clone(),
            json.clone(),
            r#"{"desk_id": "d1", "holder": "ada"}"#.into(),
        ),
        (
            "POST".into(),
            take.clone(),
            json.clone(),
            r#"{"desk_id": "d1"}"#.into(),
        ),
        (
            "POST".into(),
            take.clone(),
            json.clone(),
            r#"{"desk_id": "d1"}"#.into(),
        ),
        (
            "POST".into(),
            take.clone(),
            json.clone(),
            r#"{"desk_id": "d9"}"#.into(),
        ),
        (
            "POST".into(),
            book.clone(),
            json.clone(),
            r#"{"desk_id": "d1", "weekend": false}"#.into(),
        ),
        (
            "POST".into(),
            open.clone(),
            json.clone(),
            "{not json".into(),
        ),
        ("GET".into(), view.clone(), None, String::new()),
        (
            "GET".into(),
            format!("{view}?unknown=1"),
            None,
            String::new(),
        ),
        ("GET".into(), open, None, String::new()),
        ("GET".into(), "/no/such/route".into(), None, String::new()),
        ("GET".into(), "/api/model".into(), None, String::new()),
        ("GET".into(), "/api/events".into(), None, String::new()),
        ("GET".into(), "/api/reset".into(), None, String::new()),
        ("POST".into(), "/api/reset".into(), json, String::new()),
        ("GET".into(), view, None, String::new()),
    ]
}

#[test]
fn the_router_answers_without_a_listener() {
    let ir = ir();
    let listened_state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let routed_state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut listened = host(ir.clone(), listened_state.path());
    let mut routed = host(ir.clone(), routed_state.path());
    let listener = Listener::bind(0, surface(&ir)).expect("the listener binds");
    let address = listener.local_addr();
    let router = surface(&ir);
    let mut answers = Vec::new();

    for (method, target, content_type, body) in requests(&router) {
        let client = {
            let (method, target, content_type, body) = (
                method.clone(),
                target.clone(),
                content_type.clone(),
                body.clone(),
            );
            thread::spawn(move || send(address, &method, &target, content_type.as_deref(), &body))
        };
        for _ in 0..STEPS {
            if client.is_finished() {
                break;
            }
            if let Some(exchange) = listener.next(WAIT) {
                exchange.serve(&mut listened);
            }
        }
        assert!(
            client.is_finished(),
            "{method} {target} was not answered in time"
        );
        let over_http = client.join().expect("the client thread");

        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        let request = Request {
            method: method.clone(),
            path: path.to_owned(),
            query: query.to_owned(),
            content_type,
            body: body.into_bytes(),
        };
        let answered = spindle_ingress::route(&router, &mut routed, request);
        assert_eq!(
            answered.response, over_http,
            "{method} {target}: the router answers what the listener writes"
        );
        assert!(
            answered.failed.is_empty(),
            "{method} {target}: no command a binding invoked failed"
        );
        answers.push(answered.response);
    }
    for status in [200, 400, 404, 405] {
        assert!(
            answers.iter().any(|answer| answer.status == status),
            "the requests reach a {status}: {answers:?}"
        );
    }
    assert!(
        answers
            .iter()
            .filter(|answer| answer.status == 405)
            .all(|answer| answer.allow.is_some()),
        "every 405 names what the path answers: {answers:?}"
    );
}
