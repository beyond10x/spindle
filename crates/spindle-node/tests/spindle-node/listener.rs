//! The loopback listener answers the world exchanges: the review fixture served on 127.0.0.1 with
//! the console routes, every external outcome answered by its command's default branch as
//! `run --answer-default` answers it, each reset's world read by `run`'s reader
//! ([`spindle_host::read_world`]), every store in memory. The Node export's tests replay the same
//! record, so the two answer each request alike.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use spindle_capability::Environment;
use spindle_host::{Answers, Host, ManualClock};
use spindle_ingress::{HTTP_LOOPBACK, Listener, Surface};
use spindle_spec::EssIr;

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

const SOURCES: [(&str, &str); 2] = [
    (
        "system.yaml",
        include_str!("../../../spindle-wasm/tests/fixtures/review/system.yaml"),
    ),
    (
        "domains/review.yaml",
        include_str!("../../../spindle-wasm/tests/fixtures/review/domains/review.yaml"),
    ),
];

const WORLD: &str = include_str!("../fixtures/world-exchanges.json");
/// The record [`WORLD`] reads, which a run with [`RECORD`] set writes again.
const WORLD_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/world-exchanges.json"
);
/// Set, `the_loopback_listener_answers_the_world_exchanges` records what the listener answers
/// into [`WORLD_PATH`] in place of comparing it: `slot --fetch
/// crates/spindle-node/tests/fixtures/world-exchanges.json cargo test -p spindle-node --
/// listener::the_loopback_listener_answers_the_world_exchanges` with `SPINDLE_RECORD=1`.
const RECORD: &str = "SPINDLE_RECORD";

/// One request and the answer the loopback listener writes for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct Exchange {
    method: String,
    /// The path and query.
    target: String,
    content_type: Option<String>,
    body: String,
    status: u16,
    allow: Option<String>,
    answer: String,
}

fn ir() -> EssIr {
    let (names, texts): (Vec<String>, Vec<String>) = SOURCES
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .unzip();
    spindle_spec::compile(&names, &texts).expect("the review fixture compiles")
}

pub(crate) fn loopback() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"loopback\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
        "\"external.*\" = [\"decisions-file\"]\n",
    ))
    .expect("the loopback environment parses")
}

/// One HTTP/1.1 request, `Connection: close`: the status, the `Allow` header and the body text.
fn send(address: SocketAddr, exchange: &Exchange) -> (u16, Option<String>, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    let content_type = exchange
        .content_type
        .as_ref()
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{} {} HTTP/1.1\r\nHost: {address}\r\n{content_type}Content-Length: {}\r\n\
         Connection: close\r\n\r\n{}",
        exchange.method,
        exchange.target,
        exchange.body.len(),
        exchange.body
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
    let allow = head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case("allow")
            .then(|| value.trim().to_owned())
    });
    (status, allow, body.to_owned())
}

/// What `listener` answers each of `recorded`, each request sent on a connection of its own and
/// handed to `serve` on this thread: the record with each status, `Allow` header and body
/// replaced by the listener's.
pub(crate) fn listened(
    listener: &Listener,
    recorded: &[Exchange],
    mut serve: impl FnMut(spindle_ingress::Exchange),
) -> Vec<Exchange> {
    let address = listener.local_addr();
    let mut listened = Vec::new();
    for exchange in recorded {
        let client = {
            let exchange = exchange.clone();
            thread::spawn(move || send(address, &exchange))
        };
        for _ in 0..STEPS {
            if client.is_finished() {
                break;
            }
            if let Some(served) = listener.next(WAIT) {
                serve(served);
            }
        }
        assert!(
            client.is_finished(),
            "{} {} was not answered in time",
            exchange.method,
            exchange.target
        );
        let (status, allow, answer) = client.join().expect("the client thread");
        listened.push(Exchange {
            status,
            allow,
            answer,
            ..exchange.clone()
        });
    }
    listened
}

/// What the loopback listener answers each of `recorded`, served as the module's header says.
fn served(recorded: &[Exchange]) -> Vec<Exchange> {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let answers = Answers::default().taking_defaults(&ir);
    let mut host = Host::deciding(
        ir.clone(),
        &loopback(),
        Arc::new(ManualClock::new()),
        state.path(),
        answers.clone().served(),
    )
    .unwrap_or_else(|error| panic!("the host admits the review fixture: {error}"));
    let surface = Surface::new(&ir, &["probe".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the review surface is served: {refused}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let worlds = |ir: &EssIr, body: &Value| {
        spindle_host::read_world(ir, body, |table| table.over(answers.clone()))
    };

    listened(&listener, recorded, |served| {
        served.serve_with(&mut host, &worlds);
    })
}

#[test]
fn the_loopback_listener_answers_the_world_exchanges() {
    let recorded: Vec<Exchange> = serde_json::from_str(WORLD).expect("the record reads");
    let listened = served(&recorded);
    if std::env::var_os(RECORD).is_some() {
        let pretty = serde_json::to_string_pretty(&listened).expect("the record writes");
        std::fs::write(WORLD_PATH, format!("{pretty}\n")).expect("the record is written");
        return;
    }
    assert_eq!(
        listened,
        recorded,
        "the listener answers what tests/fixtures/world-exchanges.json records; it wrote:\n{}",
        serde_json::to_string_pretty(&listened).unwrap_or_default()
    );
}

fn exchange(method: &str, target: &str, body: &str) -> Exchange {
    Exchange {
        method: method.to_owned(),
        target: target.to_owned(),
        content_type: (!body.is_empty()).then(|| "application/json".to_owned()),
        body: body.to_owned(),
        status: 0,
        allow: None,
        answer: String::new(),
    }
}

/// story:world-row-integer-fields: a row a reset's world loads answers its `Integer` field as an
/// integer, as a row `Submit` created does.
#[test]
fn a_world_row_keeps_its_integer_fields() {
    let world = r#"{"rows": [{"entity": "probe.review.Submission", "identity": "s1", "state": "Open", "fields": {"amount": 1}}]}"#;
    let answered = served(&[
        exchange("POST", "/api/reset", world),
        exchange(
            "POST",
            "/review/commands/Submit",
            r#"{"submission_id": "s9", "amount": 12}"#,
        ),
        exchange("GET", "/review/views/Submissions", ""),
    ]);
    let statuses: Vec<u16> = answered.iter().map(|exchange| exchange.status).collect();
    assert_eq!(statuses, [200, 202, 200], "{answered:#?}");
    assert_eq!(
        answered[2].answer,
        r#"{"rows":[{"amount":1,"submission_id":"s1"},{"amount":12,"submission_id":"s9"}]}"#
    );
}
