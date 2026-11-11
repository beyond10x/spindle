//! Conformance of a listener that requires the proxy's secret (`Surface::requiring_secret`, every
//! listener `run --address` binds off loopback, story:ingress-bind-address) against its own
//! contract: a request without the secret is refused `403` "before anything else of it is read
//! or checked: its path, method, body and caller" (`Surface::requiring_secret`), "nothing of a
//! request without the secret is read: not its route, its method, its body or its caller"
//! (`listener.rs`, `answer`).
//!
//! Exercised on `::1` with a surface made to require the secret, as `exposed.rs` does, so nothing
//! listens off this host. The fixture `tests/fixtures/grants` declares one component, `ledger`.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::Value;
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "ledger";
const HEADER: &str = "x-vault-caller";
const SECRET: &str = "proxy-secret";
/// The request log's capacity on the surface served here.
const CAPACITY: usize = 8;
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

/// One request on its own connection: the status and the body as JSON.
fn exchange(address: SocketAddr, text: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .write_all(text.as_bytes())
        .expect("the request is written");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let response = String::from_utf8_lossy(&response).into_owned();
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or(0);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// The paths of the records the request log holds, newest first.
fn logged(address: SocketAddr) -> Vec<String> {
    let (status, log) = exchange(
        address,
        &format!(
            "GET /__spindle/requests HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{}: {SECRET}\r\n\r\n",
            Surface::SECRET_HEADER
        ),
    );
    assert_eq!(status, 200, "the request log with the secret: {log}");
    log["requests"]
        .as_array()
        .unwrap_or_else(|| panic!("the log lists requests: {log}"))
        .iter()
        .map(|record| record["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A request without the secret is refused before its path or method is read, so it leaves the
/// operator's request log as it was: as many requests without the secret as the log holds do not
/// push out the record of a request the proxy sent.
#[test]
fn requests_without_the_secret_leave_the_request_log_as_it_was() {
    let ir = ir();
    let surface = Surface::proving_callers(
        &ir,
        &[COMPONENT.to_owned()],
        Some(&binding(&ir)),
        HEADER,
        SECRET,
    )
    .and_then(|surface| surface.with_console(spindle_ingress::HTTP_LOOPBACK))
    .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
    .observing(CAPACITY)
    .requiring_secret();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let listener =
        Listener::bind_reading_on(IpAddr::V6(Ipv6Addr::LOCALHOST), 0, surface, host.reader())
            .expect("the listener binds ::1");
    let address = listener.local_addr();
    let (status, model) = exchange(
        address,
        &format!(
            "GET /api/model HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{}: {SECRET}\r\n\r\n",
            Surface::SECRET_HEADER
        ),
    );
    assert_eq!(status, 200, "the model with the secret: {model}");
    assert_eq!(
        logged(address),
        ["/api/model"],
        "the proxy's request is logged"
    );
    for index in 0..CAPACITY {
        let (status, body) = exchange(
            address,
            &format!(
                "GET /unproxied-{index} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert_eq!(status, 403, "a request without the secret: {body}");
    }
    assert_eq!(
        logged(address),
        ["/api/model"],
        "requests without the secret are refused before their path is read, so the log holds \
         the proxy's request and none of theirs"
    );
}
