//! Adversary, wave 28 unit U5 (`story:entity-event-history`,
//! `story:events-record-their-caller`): `GET /api/history` read with the identity a command's
//! route carries, and the request id one HTTP request records on the commands it leads to.
//!
//! The fixture `crates/spindle-host/tests/fixtures/history`: an editor writes, revises and
//! publishes records on `desk`, whose identity `record_id` is a `String`; `keep-copy` carries each
//! `RecordPublished` to `archive`'s `KeepCopy`. Every request is sent as the editor `e-7`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Caller, Request, Surface};

const EDITOR: &str = "notes.records.Editor";
const RECORD: &str = "notes.records.Record";
const COPY: &str = "notes.archive.Copy";

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
        "name = \"adversary-history\"\n",
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

struct Desk {
    surface: Surface,
    host: Host,
    _state: tempfile::TempDir,
}

impl Desk {
    fn open() -> Self {
        let ir = notes();
        let editor = Caller {
            actor: EDITOR.to_owned(),
            attributes: Map::from_iter([("editor_id".to_owned(), json!("e-7"))]),
        };
        let surface = Surface::fixing_callers(&ir, &["desk".to_owned()], None, &[editor])
            .and_then(|surface| surface.with_console(spindle_ingress::HTTP_LOOPBACK))
            .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"));
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

    /// Sends `command` with `input` through its route, as the editor; the route's path carries
    /// the fields it names.
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

fn events(body: &Value) -> Vec<Value> {
    body["events"].as_array().cloned().unwrap_or_default()
}

/// A record whose `String` identity is all digits, as an id a consumer's system mints often is,
/// is read by `identity=<the id>`, the text the command's route carried, as `identity=r1` reads
/// `r1`. The route reads it as the JSON number 42 instead, which no write of a `String` identity
/// names, and answers an empty history.
#[test]
fn a_string_identity_of_digits_reads_its_history() {
    let mut desk = Desk::open();
    desk.send("WriteRecord", json!({"record_id": "42", "text": "one"}));
    desk.send("ReviseRecord", json!({"record_id": "42", "text": "two"}));
    let (status, quoted) = desk.history(&format!("entity={RECORD}&identity=%2242%22"));
    assert_eq!(status, 200, "{quoted}");
    assert_eq!(events(&quoted).len(), 2, "quoted, it reads: {quoted}");
    let (status, bare) = desk.history(&format!("entity={RECORD}&identity=42"));
    assert_eq!(status, 200, "{bare}");
    assert_eq!(
        events(&bare),
        events(&quoted),
        "the identity as the route carried it reads the record's history: {bare}"
    );
}

/// `identity=r1` and `identity=%22r1%22` read the same history, as the route documents.
#[test]
fn a_quoted_and_a_bare_text_identity_read_alike() {
    let mut desk = Desk::open();
    desk.send("WriteRecord", json!({"record_id": "r1", "text": "one"}));
    let (_, bare) = desk.history(&format!("entity={RECORD}&identity=r1"));
    let (_, quoted) = desk.history(&format!("entity={RECORD}&identity=%22r1%22"));
    assert_eq!(events(&bare).len(), 1, "{bare}");
    assert_eq!(bare, quoted);
}

/// The command a binding ran for an HTTP request's event records the request id the ingress
/// minted for that request, beside the binding; the next HTTP request records another.
#[test]
fn a_bound_command_records_the_request_id_of_its_http_request() {
    let mut desk = Desk::open();
    desk.send("WriteRecord", json!({"record_id": "r1", "text": "one"}));
    desk.send("PublishRecord", json!({"record_id": "r1"}));
    let (status, record) = desk.history(&format!("entity={RECORD}&identity=r1"));
    assert_eq!(status, 200, "{record}");
    let record = events(&record);
    assert_eq!(record.len(), 2, "{record:?}");
    let written = record[0]["caller"]["request"]
        .as_str()
        .expect("a request id");
    let published = record[1]["caller"]["request"]
        .as_str()
        .expect("a request id");
    assert_ne!(written, published, "one request id per HTTP request");
    let (status, copy) = desk.history(&format!("entity={COPY}&identity=r1"));
    assert_eq!(status, 200, "{copy}");
    let copy = events(&copy);
    assert_eq!(copy.len(), 1, "{copy:?}");
    assert_eq!(copy[0]["cause"]["kind"], "binding", "{copy:?}");
    assert_eq!(copy[0]["cause"]["binding"], "keep-copy", "{copy:?}");
    assert_eq!(copy[0]["caller"]["actor"], EDITOR, "{copy:?}");
    assert_eq!(
        copy[0]["caller"]["request"], published,
        "the bound command records its HTTP request's id: {copy:?}"
    );
}
