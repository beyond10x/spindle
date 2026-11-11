//! A listener off loopback (`run --address`): which addresses count as loopback
//! ([`spindle_ingress::exposed`]), and the request path of a listener that requires the proxy's
//! secret on every request ([`Surface::requiring_secret`]), which every listener bound off
//! loopback is.
//!
//! No case here binds anything but `::1` and `127.0.0.1`: the request path is exercised on `::1`
//! with a surface that requires the secret, as a listener bound off loopback serves.
//!
//! The fixture `tests/fixtures/grants` declares one component, `ledger` (see `grants.rs`).

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{BODY_LIMIT, Listener, Surface};

const COMPONENT: &str = "ledger";
const HEADER: &str = "x-vault-caller";
const SECRET: &str = "proxy-secret";
/// How long the host's side waits to see whether a refused request was handed on.
const HANDED: Duration = Duration::from_millis(200);
/// How long a client waits for an answer: well under `BODY_READ`, so a request whose body the
/// listener waits for is not answered in time.
const ANSWER: Duration = Duration::from_secs(5);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("grants"))
        .unwrap_or_else(|error| panic!("the grants fixture compiles: {error:?}"))
        .ir
}

fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = std::fs::read_to_string(fixtures().join("grants-http.yaml")).expect("the binding");
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"grants\"\n",
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
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// The ledger's surface as `run --trusted-caller-header --observe` serves it: each request
/// proving its caller, the console routes and `/__spindle/` served.
fn proving(ir: &EssIr) -> Surface {
    Surface::proving_callers(
        ir,
        &[COMPONENT.to_owned()],
        Some(&binding(ir)),
        HEADER,
        SECRET,
    )
    .and_then(|surface| surface.with_console(spindle_ingress::HTTP_LOOPBACK))
    .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
    .observing(8)
}

struct Served {
    listener: Listener,
    _host: Host,
    _state: tempfile::TempDir,
}

/// `surface` served on `[::1]:<port>` with the host's reader, as `run --address ::1` serves it.
fn serve(ir: EssIr, surface: Surface) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let listener =
        Listener::bind_reading_on(IpAddr::V6(Ipv6Addr::LOCALHOST), 0, surface, host.reader())
            .expect("the listener binds ::1");
    Served {
        listener,
        _host: host,
        _state: state,
    }
}

/// `text` sent on its own connection: the status and the body as JSON, or `None` where no
/// answer arrived within [`ANSWER`].
fn exchange(address: SocketAddr, text: &str) -> Option<(u16, Value)> {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .write_all(text.as_bytes())
        .expect("the request is written");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let response = String::from_utf8(response).expect("the answer is UTF-8");
    let (head, body) = response.split_once("\r\n\r\n")?;
    let status = head.split(' ').nth(1)?.parse().ok()?;
    let body = serde_json::from_str(body).unwrap_or(Value::Null);
    Some((status, body))
}

/// Requests without the proxy's secret, one of each kind the listener answers: every console
/// route, every route under `/__spindle/`, a command and a view, and requests today's
/// trusted-caller path answers without the secret (a path no route has, a method a route does not
/// take, a chunked or oversized body, and a body that never arrives).
fn unproxied(address: SocketAddr, view: &str) -> Vec<(&'static str, String)> {
    let head = |line: &str, rest: &str| {
        format!("{line} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{rest}\r\n")
    };
    let proof = format!("{HEADER}: {{\"actor\": \"vault.ledger.Clerk\"}}\r\n");
    vec![
        ("the model", head("GET /api/model", "")),
        ("the events", head("GET /api/events", "")),
        ("a reset", head("POST /api/reset", "Content-Length: 0\r\n")),
        ("a reset under GET", head("GET /api/reset", "")),
        ("the metrics", head("GET /__spindle/metrics", "")),
        ("the requests", head("GET /__spindle/requests", "")),
        ("the topology", head("GET /__spindle/topology", "")),
        ("the panel", head("GET /__spindle/ui", "")),
        ("a path no route has", head("GET /nowhere", "")),
        (
            "a command with a caller but no secret",
            format!(
                "{}{{}}",
                head(
                    "POST /notes/1",
                    &format!("{proof}Content-Type: application/json\r\nContent-Length: 2\r\n")
                )
            ),
        ),
        ("a view", head(&format!("GET {view}"), "")),
        ("a command under GET", head("GET /notes/1", "")),
        (
            "a chunked body",
            format!(
                "{}2\r\n{{}}\r\n0\r\n\r\n",
                head(
                    "POST /notes/1",
                    "Transfer-Encoding: chunked\r\nContent-Type: application/json\r\n"
                )
            ),
        ),
        (
            "a body announced over the limit",
            head(
                "POST /notes/1",
                &format!(
                    "Content-Type: application/json\r\nContent-Length: {}\r\n",
                    BODY_LIMIT + 1
                ),
            ),
        ),
        (
            "a body that never arrives",
            head(
                "POST /notes/1",
                "Content-Type: application/json\r\nContent-Length: 10\r\n",
            ),
        ),
    ]
}

#[test]
fn only_127_0_0_0_8_and_ipv6_loopback_are_loopback() {
    for loopback in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V4(Ipv4Addr::new(127, 255, 255, 254)),
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        assert!(
            !spindle_ingress::exposed(loopback),
            "{loopback} is loopback"
        );
    }
    for exposed in [
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        IpAddr::V4(Ipv4Addr::new(128, 0, 0, 1)),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
        IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()),
    ] {
        assert!(
            spindle_ingress::exposed(exposed),
            "{exposed} is not loopback"
        );
    }
}

#[test]
fn a_listener_requiring_the_secret_refuses_every_request_without_it_before_reading_it() {
    let ir = ir();
    let surface = proving(&ir).requiring_secret();
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "vault.ledger.AllEntries")
        .map(|route| route.path.clone())
        .expect("the surface serves AllEntries");
    let served = serve(ir, surface);
    let address = served.listener.local_addr();
    assert_eq!(address.ip(), IpAddr::V6(Ipv6Addr::LOCALHOST));
    for (what, text) in unproxied(address, &view) {
        let answered = exchange(address, &text);
        assert_eq!(
            answered,
            Some((403, json!({"refused": "not granted", "actor": null}))),
            "{what} without the secret is refused 403 before anything is read"
        );
        assert!(
            served.listener.next(HANDED).is_none(),
            "{what} without the secret is not handed to the host"
        );
    }
    let model = format!(
        "GET /api/model HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{}: {SECRET}\r\n\r\n",
        Surface::SECRET_HEADER
    );
    let (status, body) = exchange(address, &model).expect("the model is answered");
    assert_eq!(status, 200, "the model with the secret: {body}");
}

#[test]
fn a_listener_not_requiring_the_secret_on_loopback_answers_as_before() {
    let ir = ir();
    let surface = proving(&ir);
    let served = serve(ir, surface);
    let address = served.listener.local_addr();
    let nowhere = format!("GET /nowhere HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    let (status, body) = exchange(address, &nowhere).expect("an answer");
    assert_eq!(
        status, 404,
        "a path no route has, on loopback as before: {body}"
    );
    let model = format!("GET /api/model HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    assert_eq!(
        exchange(address, &model),
        Some((403, json!({"refused": "not granted", "actor": null}))),
        "a console route without the secret, on loopback as before"
    );
}

#[test]
fn a_surface_without_a_secret_is_not_bound_where_one_is_required() {
    let ir = ir();
    let open = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&binding(&ir)))
        .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let required = Listener::bind_reading_on(
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        0,
        open.clone().requiring_secret(),
        host.reader(),
    );
    assert_eq!(
        required.map(|_| ()).map_err(|error| error.kind()),
        Err(ErrorKind::InvalidInput),
        "a surface that requires a secret it does not have is refused"
    );
    // A documentation address (RFC 5737): refused before any bind, and no host holds it.
    let exposed = Listener::bind_reading_on(
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        0,
        open,
        host.reader(),
    );
    assert_eq!(
        exposed.map(|_| ()).map_err(|error| error.kind()),
        Err(ErrorKind::InvalidInput),
        "a surface without the proxy's secret is not bound off loopback"
    );
}
