//! Adversary cases for `story:query-input-beside-body` (wave 27, unit U3): a command route whose
//! `ess-http/1` binding lists `request.query`, held to ESS 0.99.1's served application
//! (`ess-synth` `http/bound.rs`: `json_body` or the bare body, `with_path`, then
//! `flat_object(query, <listed rows>, &[], unknown, "query")`, then `with_query`) and to ESS's
//! interpreter (`ess_conformance::interpret`) for the joined input.
//!
//! The model declares one component, `board`, reached over the network, whose five `Pin` commands each
//! take a `holder`, a `label`, an instant `at`, an integer `count` and an optional `note`, and
//! publish `Pinned` with every one of them, so the answer shows the input the command ran on.
//! [`ROUTES`] binds:
//!
//! - `Pin`, `POST /pins`: a `json` body, `at`, `count` and `note` from the query string;
//! - `PinBare`, `POST /bare`: the bare `json` body is `label` (`request.body`), every other field
//!   from the query string, as ESS-HTTP-020 requires of a bare body beside `request.query`;
//! - `PinHeld`, `POST /held/{holder}`: `holder` from the path, `count` from the query string;
//! - `PinLoose`, `POST /loose`: a `json` body under `unknown: ignore`, `count` from the query;
//! - `PinPlain`, `POST /plain`: a `json` body and no `request.query`;
//! - `Tag`, `POST /tags/{holder}`: the bare `json` body is `label`, `holder` from the path, and
//!   no `request.query`.

use std::collections::BTreeMap;
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Request, Response, Surface};

const COMPONENT: &str = "board";
const JSON: &str = "application/json";
const AT: &str = "2026-11-26T10:00:00Z";

const SYSTEM: &str = "format: ess/34
system: board
version: v1
summary: Pins put on a board over the network, read from the body and the query string.
domains:
  - board.pin
components:
  - component: board
    summary: Puts pins on the board for callers outside the process.
    owns:
      domains:
        - board.pin
    accepts:
      commands:
        - board.pin.Pin
        - board.pin.PinBare
        - board.pin.PinHeld
        - board.pin.PinLoose
        - board.pin.PinPlain
        - board.pin.Tag
    publishes:
      events:
        - board.pin.Pinned
        - board.pin.Tagged
    reached_by: network
";

const DOMAIN: &str = "domain: board.pin
summary: Pins, each published with every field it was given.
commands:
  - name: board.pin.Pin
    input:
      - {name: holder, type: String}
      - {name: label, type: String}
      - {name: at, type: Timestamp}
      - {name: count, type: Integer}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: pinned
        emits: [board.pin.Pinned]
        payload:
          board.pin.Pinned: {holder: input.holder, label: input.label, at: input.at, count: input.count, note: input.note}
        summary: The pin is published with every field it was given.
  - name: board.pin.PinBare
    input:
      - {name: holder, type: String}
      - {name: label, type: String}
      - {name: at, type: Timestamp}
      - {name: count, type: Integer}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: pinned
        emits: [board.pin.Pinned]
        payload:
          board.pin.Pinned: {holder: input.holder, label: input.label, at: input.at, count: input.count, note: input.note}
        summary: The pin is published with every field it was given.
  - name: board.pin.PinHeld
    input:
      - {name: holder, type: String}
      - {name: label, type: String}
      - {name: at, type: Timestamp}
      - {name: count, type: Integer}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: pinned
        emits: [board.pin.Pinned]
        payload:
          board.pin.Pinned: {holder: input.holder, label: input.label, at: input.at, count: input.count, note: input.note}
        summary: The pin is published with every field it was given.
  - name: board.pin.PinLoose
    input:
      - {name: holder, type: String}
      - {name: label, type: String}
      - {name: at, type: Timestamp}
      - {name: count, type: Integer}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: pinned
        emits: [board.pin.Pinned]
        payload:
          board.pin.Pinned: {holder: input.holder, label: input.label, at: input.at, count: input.count, note: input.note}
        summary: The pin is published with every field it was given.
  - name: board.pin.PinPlain
    input:
      - {name: holder, type: String}
      - {name: label, type: String}
      - {name: at, type: Timestamp}
      - {name: count, type: Integer}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: pinned
        emits: [board.pin.Pinned]
        payload:
          board.pin.Pinned: {holder: input.holder, label: input.label, at: input.at, count: input.count, note: input.note}
        summary: The pin is published with every field it was given.
  - name: board.pin.Tag
    input:
      - {name: holder, type: String}
      - {name: label, type: String}
    outcomes:
      - name: tagged
        emits: [board.pin.Tagged]
        payload:
          board.pin.Tagged: {holder: input.holder, label: input.label}
        summary: The tag is published with both fields it was given.
events:
  - name: board.pin.Tagged
    fields:
      - {name: holder, type: String}
      - {name: label, type: String}
  - name: board.pin.Pinned
    fields:
      - {name: holder, type: String}
      - {name: label, type: String}
      - {name: at, type: Timestamp}
      - {name: count, type: Integer}
      - {name: note, type: Optional<String>}
";

const ROUTES: &str = "  - command: board.pin.Pin
    method: post
    path: /pins
    request: {encoding: json, query: [input.at, input.count, input.note]}
    outcomes:
      pinned: {status: 200}
  - command: board.pin.PinBare
    method: post
    path: /bare
    request: {encoding: json, body: input.label, query: [input.holder, input.at, input.count, input.note]}
    outcomes:
      pinned: {status: 200}
  - command: board.pin.PinHeld
    method: post
    path: /held/{holder}
    parameters: {holder: input.holder}
    request: {encoding: json, query: [input.count]}
    outcomes:
      pinned: {status: 200}
  - command: board.pin.PinLoose
    method: post
    path: /loose
    request: {encoding: json, unknown: ignore, query: [input.count]}
    outcomes:
      pinned: {status: 200}
  - command: board.pin.PinPlain
    method: post
    path: /plain
    outcomes:
      pinned: {status: 200}
  - command: board.pin.Tag
    method: post
    path: /tags/{holder}
    parameters: {holder: input.holder}
    request: {encoding: json, body: input.label}
    outcomes:
      tagged: {status: 200}
";

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-w27-u3\"\n",
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

fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = format!(
        "type: ess-http/1
specification:
  system: board
  version: v1
  source_digest: sha256:{}
routes:
{ROUTES}",
        ir.source_digest()
    );
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}\n{text}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

struct Board {
    surface: Surface,
    host: Host,
    interpreter: Interpreted,
    _spec: tempfile::TempDir,
    _state: tempfile::TempDir,
}

impl Board {
    fn open() -> Self {
        let spec = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a spec directory");
        std::fs::create_dir(spec.path().join("domains")).expect("the domains directory");
        std::fs::write(spec.path().join("system.yaml"), SYSTEM).expect("the system");
        std::fs::write(spec.path().join("domains/pin.yaml"), DOMAIN).expect("the domain");
        let ir = spindle_spec::load_model(spec.path())
            .unwrap_or_else(|error| panic!("the board compiles: {error:?}"))
            .ir;
        let bound = binding(&ir);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&bound))
            .unwrap_or_else(|refused| panic!("the board surface is routed: {refused}"));
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(
            ir.clone(),
            &in_memory(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("the host admits the board: {error}"));
        Self {
            surface,
            host,
            interpreter: Interpreted::for_model(ir),
            _spec: spec,
            _state: state,
        }
    }

    fn post(&mut self, target: &str, body: &str) -> Response {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: "POST".to_owned(),
                path: path.to_owned(),
                query: query.to_owned(),
                content_type: Some(JSON.to_owned()),
                body: body.as_bytes().to_vec(),
            },
        )
        .response
    }

    /// What ESS's interpreter publishes for `command` run on `input`; `Err` where it refuses the
    /// input.
    fn interpreted(&self, command: &str, input: Value) -> Result<Vec<Value>, String> {
        let input: BTreeMap<String, Node> =
            serde_json::from_value(input).map_err(|error| format!("not ESS values: {error}"))?;
        let result = self
            .interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input,
                correlation: CorrelationId::new("adversary-w27-u3").expect("a correlation"),
            })
            .map_err(|error| format!("{error:?}"))?;
        if result.direct_events.is_empty() {
            return Err(format!("no event: {:?}", result.outcome));
        }
        Ok(result
            .direct_events
            .iter()
            .map(|event| serde_json::to_value(&event.payload).expect("a payload as JSON"))
            .collect())
    }
}

fn parsed(response: &Response) -> Value {
    serde_json::from_str(&response.body)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", response.body))
}

fn published(response: &Response) -> Vec<Value> {
    let body = parsed(response);
    body["published"]
        .as_array()
        .unwrap_or_else(|| panic!("published events: {body}"))
        .iter()
        .map(|event| event["payload"].clone())
        .collect()
}

/// `values` with every number written as an `f64`: ESS's interpreter writes an `Integer` payload
/// field as `1.0` where Spindle writes `1`, the same number.
fn numeric(values: &[Value]) -> Vec<Value> {
    fn one(value: &Value) -> Value {
        match value {
            Value::Number(number) => json!(number.as_f64()),
            Value::Array(items) => Value::Array(items.iter().map(one).collect()),
            Value::Object(members) => Value::Object(
                members
                    .iter()
                    .map(|(key, value)| (key.clone(), one(value)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    values.iter().map(one).collect()
}

/// `response` ran `command` on `joined` and published what ESS's interpreter publishes for it.
fn ran_as_interpreted(board: &Board, response: &Response, command: &str, joined: Value) {
    let expected = board
        .interpreted(command, joined.clone())
        .unwrap_or_else(|refused| panic!("the interpreter runs {command} on {joined}: {refused}"));
    assert_eq!(
        response.status, 200,
        "{command} on {joined}: {}",
        response.body
    );
    assert_eq!(
        numeric(&published(response)),
        numeric(&expected),
        "{command} on {joined}"
    );
}

/// `response` is a `400` whose `refused` text holds `detail`.
fn refused(response: &Response, detail: &str) {
    assert_eq!(response.status, 400, "{detail:?}: {}", response.body);
    let refused = parsed(response)["refused"].clone();
    assert!(
        refused.as_str().is_some_and(|text| text.contains(detail)),
        "{detail:?} in {refused}"
    );
}

/// Every route of [`ROUTES`] is served: none keeps the 501 of a shape this ingress does not read.
#[test]
fn every_route_with_request_query_is_served() {
    let board = Board::open();
    let unserved: Vec<String> = board
        .surface
        .unserved()
        .iter()
        .filter_map(|route| route.not_served())
        .collect();
    assert!(unserved.is_empty(), "{unserved:?}");
}

/// ESS 0.99.1 reads a route's bare body (`request.body: input.label`) as that one field's value
/// (`ess-synth` `http/bound.rs`: `json::Value::Object(vec![("label", http::json_body(..))])`),
/// joined with the query's fields. A request whose body is the JSON text `"hello"` runs the
/// command on `label: "hello"`. Before this unit the route answered 501 (`request.query`).
#[test]
fn a_bare_body_beside_the_query_is_read_as_its_one_field() {
    let mut board = Board::open();
    let response = board.post(&format!("/bare?holder=h1&at={AT}&count=3"), r#""hello""#);
    ran_as_interpreted(
        &board,
        &response,
        "board.pin.PinBare",
        json!({"holder": "h1", "label": "hello", "at": AT, "count": 3}),
    );
}

/// The bare body on a route without `request.query`, `Tag` at `POST /tags/{holder}`: ESS reads
/// the body `"hello"` as `label`. Runs on code the unit did not change; it shows the bare body was
/// not read before this unit either, where the path carries every other field.
#[test]
fn a_bare_body_beside_the_path_is_read_as_its_one_field() {
    let mut board = Board::open();
    let response = board.post("/tags/h1", r#""hello""#);
    ran_as_interpreted(
        &board,
        &response,
        "board.pin.Tag",
        json!({"holder": "h1", "label": "hello"}),
    );
}

/// ESS reads the query string (`flat_object`) before it refuses a body member a listed field
/// names (`with_query`): where both are wrong, the query's refusal is the answer.
#[test]
fn the_query_refusal_wins_over_a_body_naming_a_queried_field() {
    let mut board = Board::open();
    let body = json!({"holder": "h1", "label": "l1", "count": 1}).to_string();
    for (target, detail) in [
        (
            format!("/pins?at={AT}&count=1&count=2"),
            "query.count: expected one value, found 2",
        ),
        (
            format!("/pins?at={AT}&count=1&origin=web"),
            "query.origin: no field of this request is named so",
        ),
        (
            format!("/pins?at={AT}&count="),
            "query.count: expected a value, found the empty text",
        ),
    ] {
        let response = board.post(&target, &body);
        refused(&response, detail);
    }
}

/// The same route given an object body: ESS reads the whole object as `label`, a `String`, and
/// its decoder refuses it `400`; it never runs the command on the object's members.
#[test]
fn an_object_body_on_a_bare_route_is_not_read_as_members() {
    let mut board = Board::open();
    let response = board.post(
        &format!("/bare?holder=h1&at={AT}&count=3"),
        r#"{"label":"hello"}"#,
    );
    assert_eq!(
        response.status, 400,
        "ESS's server reads the object as the value of `label` and refuses it: {}",
        response.body
    );
}

/// Query values are form-decoded as ESS's `form_decoded` decodes them: `+` a space, `%2B` a plus,
/// `%26` and `%3D` themselves, a key percent-encoded (`%63ount` is `count`), and an `Integer` in
/// exact decimal text with leading zeros the number it spells.
#[test]
fn query_values_decode_as_ess_decodes_a_form() {
    let mut board = Board::open();
    let body = r#"{"holder":"h1","label":"l1"}"#;
    let response = board.post(
        &format!("/pins?at={AT}&%63ount=007&note=a+b%2Bc%26d%3D"),
        body,
    );
    ran_as_interpreted(
        &board,
        &response,
        "board.pin.Pin",
        json!({"holder": "h1", "label": "l1", "at": AT, "count": 7, "note": "a b+c&d="}),
    );
    let negative = board.post(&format!("/pins?at={AT}&count=-12"), body);
    ran_as_interpreted(
        &board,
        &negative,
        "board.pin.Pin",
        json!({"holder": "h1", "label": "l1", "at": AT, "count": -12}),
    );
}

/// An empty value, or a key without `=`, is the empty text: a `String` field reads it as `""`,
/// any other field is refused `query.<key>: expected a value, found the empty text`
/// (`http::flat_value`).
#[test]
fn an_empty_query_value_is_the_empty_text() {
    let mut board = Board::open();
    let body = r#"{"holder":"h1","label":"l1"}"#;
    for target in [
        format!("/pins?at={AT}&count=1&note="),
        format!("/pins?at={AT}&count=1&note"),
    ] {
        let response = board.post(&target, body);
        ran_as_interpreted(
            &board,
            &response,
            "board.pin.Pin",
            json!({"holder": "h1", "label": "l1", "at": AT, "count": 1, "note": ""}),
        );
    }
    for target in [
        format!("/pins?at={AT}&count="),
        format!("/pins?at={AT}&count"),
    ] {
        let response = board.post(&target, body);
        refused(
            &response,
            "query.count: expected a value, found the empty text",
        );
    }
}

/// A query value its field's type does not read is refused `400`, as ESS's decoder refuses the
/// text `http::flat_value` leaves it: an `Integer` not in exact decimal text, an invalid escape,
/// an absent required field. The interpreter refuses each joined input too.
#[test]
fn a_query_value_its_type_refuses_answers_400() {
    let mut board = Board::open();
    let body = r#"{"holder":"h1","label":"l1"}"#;
    for count in ["1.5", "%2B5", "x", "1e3"] {
        let response = board.post(&format!("/pins?at={AT}&count={count}"), body);
        assert_eq!(response.status, 400, "count={count}: {}", response.body);
    }
    let escape = board.post(&format!("/pins?at={AT}&count=1&note=%ZZ"), body);
    refused(&escape, "query.note: expected percent-encoded UTF-8 text");
    let key = board.post(&format!("/pins?at={AT}&count=1&%ZZ=1"), body);
    refused(&key, "query: a key is not percent-encoded UTF-8");
    let missing = board.post("/pins?count=1", body);
    assert_eq!(missing.status, 400, "{}", missing.body);
}

/// A `Timestamp` read from the query string is held to what ESS's interpreter accepts for it: the
/// joined input it refuses is answered `400`, not run.
#[test]
fn a_query_instant_is_held_as_the_interpreter_holds_it() {
    let mut board = Board::open();
    let body = r#"{"holder":"h1","label":"l1"}"#;
    for at in [
        "2026-11-26T10:00:00",
        "2026-02-30T10:00:00Z",
        "2026-11-26",
        "yesterday",
    ] {
        let joined = json!({"holder": "h1", "label": "l1", "at": at, "count": 1});
        let response = board.post(&format!("/pins?at={at}&count=1"), body);
        match board.interpreted("board.pin.Pin", joined.clone()) {
            Ok(expected) => {
                assert_eq!(response.status, 200, "at={at}: {}", response.body);
                assert_eq!(
                    numeric(&published(&response)),
                    numeric(&expected),
                    "at={at}"
                );
            }
            Err(refusal) => assert_eq!(
                response.status, 400,
                "at={at}: the interpreter refuses ({refusal}), and Spindle answered {}",
                response.body
            ),
        }
    }
}

/// The same instants in a `json` body on a route without `request.query`: whatever the query
/// route answers, a body route answers alike (this case runs on code the unit did not change).
#[test]
fn a_body_instant_is_held_as_the_interpreter_holds_it() {
    let mut board = Board::open();
    for at in [
        "2026-11-26T10:00:00",
        "2026-02-30T10:00:00Z",
        "2026-11-26",
        "yesterday",
    ] {
        let joined = json!({"holder": "h1", "label": "l1", "at": at, "count": 1});
        let response = board.post("/plain", &joined.to_string());
        match board.interpreted("board.pin.PinPlain", joined.clone()) {
            Ok(expected) => {
                assert_eq!(response.status, 200, "at={at}: {}", response.body);
                assert_eq!(
                    numeric(&published(&response)),
                    numeric(&expected),
                    "at={at}"
                );
            }
            Err(refusal) => assert_eq!(
                response.status, 400,
                "at={at}: the interpreter refuses ({refusal}), and Spindle answered {}",
                response.body
            ),
        }
    }
}

/// ESS checks the path's join (`http::with_path`) before it reads the query string: a body naming
/// the field the path carries is refused for that, naming it, even where the query string is
/// malformed too, or the body also names a field the query carries.
#[test]
fn the_path_join_is_refused_before_the_query_is_read() {
    let mut board = Board::open();
    for (target, body) in [
        (
            "/held/h1?count=1&count=2".to_owned(),
            json!({"holder": "h1", "label": "l1", "at": AT}),
        ),
        (
            "/held/h1?count=1".to_owned(),
            json!({"holder": "h1", "label": "l1", "at": AT, "count": 1}),
        ),
    ] {
        let response = board.post(&target, &body.to_string());
        assert_eq!(response.status, 400, "{target}: {}", response.body);
        let text = parsed(&response)["refused"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            text.contains("holder") && !text.contains("count"),
            "{target} {body}: ESS refuses `body.holder: the path carries this field, so the \
             request does not` first; Spindle refused {text:?}"
        );
    }
}

/// On a `json` route under `unknown: ignore`, a body member under a listed field's wire name is
/// kept by ESS's `ignoring` (every field but the path's is in its wire table) and refused by
/// `with_query`; an unknown member and an unknown query key are dropped, and so is a query key
/// naming a body field.
#[test]
fn an_ignoring_json_route_drops_the_unknown_and_refuses_the_queried() {
    let mut board = Board::open();
    let named = board.post(
        "/loose?count=1",
        &json!({"holder": "h1", "label": "l1", "at": AT, "count": 1}).to_string(),
    );
    refused(
        &named,
        "body.count: the query carries this field, so the body does not",
    );
    let silent = board.post(
        "/loose",
        &json!({"holder": "h1", "label": "l1", "at": AT, "count": 1}).to_string(),
    );
    refused(
        &silent,
        "body.count: the query carries this field, so the body does not",
    );
    let dropped = board.post(
        "/loose?count=2&origin=web&holder=h9&%ZZ=1",
        &json!({"holder": "h1", "label": "l1", "at": AT, "extra": true}).to_string(),
    );
    ran_as_interpreted(
        &board,
        &dropped,
        "board.pin.PinLoose",
        json!({"holder": "h1", "label": "l1", "at": AT, "count": 2}),
    );
}

/// A route without `request.query` keeps its bytes: its query string is not read, whatever it
/// holds.
#[test]
fn a_route_without_request_query_reads_no_query_string() {
    let mut board = Board::open();
    let joined = json!({"holder": "h1", "label": "l1", "at": AT, "count": 4});
    let response = board.post("/plain?count=x&count=y&%ZZ&note=n", &joined.to_string());
    ran_as_interpreted(&board, &response, "board.pin.PinPlain", joined);
}

/// A body that is not a JSON object beside a query field: `400` either way, never run.
#[test]
fn a_body_that_is_no_object_is_refused_beside_the_query() {
    let mut board = Board::open();
    for body in ["[]", "null", "\"h1\"", "{"] {
        let response = board.post(&format!("/pins?at={AT}&count=1"), body);
        assert_eq!(response.status, 400, "{body}: {}", response.body);
    }
}
