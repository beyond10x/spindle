//! `GET /api/history?entity=&identity=&after=&limit=` (`story:entity-event-history`): the
//! committed events of one entity instance, oldest first, each with its position, the time of
//! its command, the command, its caller and its outcome, paged as `GET /api/events` pages; a
//! malformed query refused `400`; the identity elided in the request log. Each HTTP request is
//! recorded with the caller it was sent as and a request id the ingress minted for it
//! (`story:events-record-their-caller`).
//!
//! The fixture `crates/spindle-host/tests/fixtures/history`: an editor (`notes.records.Editor`,
//! attribute `editor_id`) writes and revises records on `desk`; every request here is sent as
//! the editor `e-7` (`Surface::fixing_callers`).

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Answer, Caller, Listener, Request, Surface, console};

const EDITOR: &str = "notes.records.Editor";
const RECORD: &str = "notes.records.Record";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn notes() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/history"))
        .unwrap_or_else(|error| panic!("the history fixture compiles: {error:?}"))
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"history\"\n",
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
    .expect("the environment parses")
}

fn editor() -> Caller {
    Caller {
        actor: EDITOR.to_owned(),
        attributes: Map::from_iter([("editor_id".to_owned(), json!("e-7"))]),
    }
}

/// The desk's surface with the console routes, every operation sent as the editor.
fn surface(ir: &ess_compiler::EssIr) -> Surface {
    Surface::fixing_callers(ir, &["desk".to_owned()], None, &[editor()])
        .and_then(|surface| surface.with_console(spindle_ingress::HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"))
}

struct Desk {
    surface: Surface,
    host: Host,
    _state: tempfile::TempDir,
}

impl Desk {
    fn open() -> Self {
        let ir = notes();
        let surface = surface(&ir);
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
        Self {
            surface,
            host,
            _state: state,
        }
    }

    fn routed(&mut self, method: &str, path: &str, query: &str, body: &[u8]) -> (u16, Value) {
        let routed = spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: method.to_owned(),
                path: path.to_owned(),
                query: query.to_owned(),
                content_type: Some("application/json".to_owned()),
                body: body.to_vec(),
            },
        );
        let body = if routed.response.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&routed.response.body)
                .unwrap_or_else(|_| panic!("not JSON: {}", routed.response.body))
        };
        (routed.response.status, body)
    }

    /// Sends `command` with `input` through its route, as the editor.
    fn send(&mut self, command: &str, input: Value) {
        let name = format!("notes.records.{command}");
        let route = self
            .surface
            .routes()
            .iter()
            .find(|route| route.operation.name() == name)
            .unwrap_or_else(|| panic!("the surface serves {name}"));
        let (method, mut path) = (route.method.as_str().to_uppercase(), route.path.clone());
        let mut body = input.as_object().cloned().unwrap_or_default();
        for (field, value) in input.as_object().into_iter().flatten() {
            let variable = format!("{{{field}}}");
            if path.contains(&variable) {
                path = path.replace(&variable, value.as_str().unwrap_or_default());
                body.remove(field);
            }
        }
        let (status, answer) = self.routed(
            &method,
            &path,
            "",
            Value::Object(body).to_string().as_bytes(),
        );
        assert!(status < 300, "{name}: {status} {answer}");
    }

    fn history(&mut self, query: &str) -> (u16, Value) {
        self.routed("GET", "/api/history", query, b"")
    }
}

/// Three commands change `r1`, one changes `r2`, each its own HTTP request.
fn script(desk: &mut Desk) {
    desk.send("WriteRecord", json!({"record_id": "r1", "text": "one"}));
    desk.send("ReviseRecord", json!({"record_id": "r1", "text": "two"}));
    desk.send("WriteRecord", json!({"record_id": "r2", "text": "other"}));
    desk.send("ReviseRecord", json!({"record_id": "r1", "text": "three"}));
}

fn events(body: &Value) -> Vec<Value> {
    body["events"].as_array().cloned().unwrap_or_default()
}

/// `GET /api/history` answers the events of one instance as JSON, oldest first, each with its
/// position, time, command, outcome and caller, the caller's request id one the ingress minted
/// for its HTTP request; paged after `next`, on the host's thread and through its reader alike.
#[test]
fn instance_history_route_answers_its_events() {
    let mut desk = Desk::open();
    script(&mut desk);
    let query = format!("entity={RECORD}&identity=r1");
    let (status, body) = desk.history(&query);
    assert_eq!(status, 200, "{body}");
    let listed = events(&body);
    let said: Vec<(&str, &str, &str)> = listed
        .iter()
        .map(|event| {
            (
                event["command"].as_str().unwrap_or_default(),
                event["outcome"].as_str().unwrap_or_default(),
                event["event"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            (
                "notes.records.WriteRecord",
                "written",
                "notes.records.RecordWritten"
            ),
            (
                "notes.records.ReviseRecord",
                "revised",
                "notes.records.RecordRevised"
            ),
            (
                "notes.records.ReviseRecord",
                "revised",
                "notes.records.RecordRevised"
            ),
        ],
        "{body}"
    );
    let mut requests = BTreeSet::new();
    for event in &listed {
        assert!(event["position"].as_u64().is_some(), "{event}");
        assert!(
            event["at"].as_str().is_some_and(|at| at.ends_with('Z')),
            "{event}"
        );
        assert_eq!(event["payload"]["record_id"], "r1", "{event}");
        let caller = &event["caller"];
        assert_eq!(caller["actor"], EDITOR, "{event}");
        assert_eq!(caller["attributes"], json!({"editor_id": "e-7"}), "{event}");
        let request = caller["request"].as_str().expect("a request id");
        assert!(!request.is_empty(), "{event}");
        requests.insert(request.to_owned());
    }
    assert_eq!(
        requests.len(),
        3,
        "one request id per HTTP request: {requests:?}"
    );

    let (status, page) = desk.history(&format!("{query}&limit=2"));
    assert_eq!(status, 200, "{page}");
    assert_eq!(events(&page), listed[..2], "{page}");
    let next = page["next"].as_str().expect("a cursor").to_owned();
    let (status, rest) = desk.history(&format!("{query}&limit=2&after={next}"));
    assert_eq!(status, 200, "{rest}");
    assert_eq!(events(&rest), listed[2..], "{rest}");

    let reader = desk.host.reader();
    let beside = query.clone();
    let answer: Answer =
        std::thread::spawn(move || console::history_read_queried(&reader, &beside))
            .join()
            .expect("the read ends");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.body, Some(body), "the reader answers it alike");

    let (status, empty) = desk.history(&format!("entity={RECORD}&identity=r9"));
    assert_eq!(status, 200, "{empty}");
    assert_eq!(events(&empty), Vec::<Value>::new());
}

/// A query without an entity or an identity, one naming no entity of the model, a parameter given
/// twice, a malformed `after` or `limit` are refused `400`, naming the parameter and never its
/// value.
#[test]
fn a_malformed_history_query_is_refused() {
    let mut desk = Desk::open();
    script(&mut desk);
    let identity = "identity=secret-r1";
    let malformed = [
        (identity.to_string(), "entity"),
        (format!("entity={RECORD}"), "identity"),
        (format!("entity=secret.Nothing&{identity}"), "entity"),
        (format!("entity={RECORD}&{identity}&{identity}"), "identity"),
        (
            format!("entity={RECORD}&{identity}&after=secret-cursor"),
            "after",
        ),
        (format!("entity={RECORD}&{identity}&limit=0"), "limit"),
        (format!("entity={RECORD}&{identity}&limit=secret"), "limit"),
    ];
    for (query, parameter) in malformed {
        let (status, body) = desk.history(&query);
        assert_eq!(status, 400, "{query}: {body}");
        let refused = body["refused"].as_str().unwrap_or_default();
        assert!(
            refused.contains(&format!("query.{parameter}")),
            "{query}: names the parameter: {body}"
        );
        assert!(
            !refused.contains("secret"),
            "{query}: no value echoed: {body}"
        );
    }
}

/// One `Connection: close` request: the status and the body as JSON.
fn send(address: SocketAddr, target: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body}"));
    (status, body)
}

/// The request log keeps a history read's cursor and limit, and writes the entity and the
/// identity `…`.
#[test]
fn the_request_log_elides_the_identity_of_a_history_read() {
    let mut desk = Desk::open();
    script(&mut desk);
    let (_, page) = desk.history(&format!("entity={RECORD}&identity=r1&limit=1"));
    let cursor = page["next"].as_str().expect("a cursor").to_owned();
    let observed = surface(desk.host.ir()).observing(16);
    let listener =
        Listener::bind_reading(0, observed, desk.host.reader()).expect("the listener binds");
    let address = listener.local_addr();
    let (status, body) = send(
        address,
        &format!("/api/history?entity={RECORD}&identity=secret-r1&after={cursor}&limit=1"),
    );
    assert_eq!(status, 200, "{body}");
    let (status, log) = send(address, "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    let record = &log["requests"][0];
    assert_eq!(record["path"], "/api/history", "{record}");
    assert_eq!(
        record["query"],
        format!("…=…&…=…&after={cursor}&limit=1"),
        "{record}"
    );
    assert!(!record.to_string().contains("secret"), "{record}");
}
