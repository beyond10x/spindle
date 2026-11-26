//! A command route whose `ess-http/1` binding lists `request.query` (beyond10x/ess#754), served by
//! [`spindle_ingress::route`] as ESS 0.99.1's served application reads it
//! (`story:query-input-beside-body`): each listed field is read from the query string under its
//! wire name as `encoding: query` reads it (`http::flat_object`), a `List<T>` field from its key
//! repeated, and joined with the body's fields (`http::with_query`); the body naming a listed
//! field is refused `400`, whether the query carries it or not, and so is a query key no listed
//! field names under `unknown: refuse`. The joined input runs as ESS's interpreter runs it.
//!
//! The model, [`SYSTEM`] and [`DOMAIN`], declares one component, `depot`, reached over the
//! network: `Restock` and `Recount` take a `batch_id`, a `dry_run` (wire name `dryRun`), a
//! `tags` list and an optional `note`, and each publishes `Restocked` carrying every one of them,
//! so the answer's envelope shows the input the command ran on. [`ROUTES`] reads `Restock` from a
//! `json` body with `dry_run` and `tags` from the query string, and `Recount` from a `form` body,
//! ignoring unknown keys, with `dry_run` and `note` from the query string.

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

const COMPONENT: &str = "depot";
const RESTOCK: &str = "depot.stock.Restock";
const RECOUNT: &str = "depot.stock.Recount";
const JSON: &str = "application/json";
const FORM: &str = "application/x-www-form-urlencoded";
const CARRIED: &str = ": the query carries this field, so the body does not";

const SYSTEM: &str = "format: ess/34
system: depot
version: v1
summary: Batches restocked and recounted over the network, read from the body and the query string.
domains:
  - depot.stock
components:
  - component: depot
    summary: Restocks and recounts batches for callers outside the process.
    owns:
      domains:
        - depot.stock
    accepts:
      commands:
        - depot.stock.Restock
        - depot.stock.Recount
    publishes:
      events:
        - depot.stock.Restocked
    reached_by: network
";

const DOMAIN: &str = "domain: depot.stock
summary: Restocks and recounts, each published with every field it was given.
commands:
  - name: depot.stock.Restock
    input:
      - {name: batch_id, type: String}
      - {name: dry_run, type: Boolean, wire: dryRun}
      - {name: tags, type: List<String>}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: restocked
        emits: [depot.stock.Restocked]
        payload:
          depot.stock.Restocked: {batch_id: input.batch_id, dry_run: input.dry_run, tags: input.tags, note: input.note}
        summary: The restock is published with every field it was given.
  - name: depot.stock.Recount
    input:
      - {name: batch_id, type: String}
      - {name: dry_run, type: Boolean, wire: dryRun}
      - {name: tags, type: List<String>}
      - {name: note, type: Optional<String>}
    outcomes:
      - name: recounted
        emits: [depot.stock.Restocked]
        payload:
          depot.stock.Restocked: {batch_id: input.batch_id, dry_run: input.dry_run, tags: input.tags, note: input.note}
        summary: The recount is published with every field it was given.
events:
  - name: depot.stock.Restocked
    fields:
      - {name: batch_id, type: String}
      - {name: dry_run, type: Boolean}
      - {name: tags, type: List<String>}
      - {name: note, type: Optional<String>}
";

const ROUTES: &str = "  - command: depot.stock.Restock
    method: post
    path: /restock
    request: {encoding: json, query: [input.dry_run, input.tags]}
    outcomes:
      restocked: {status: 200}
  - command: depot.stock.Recount
    method: post
    path: /recount
    request: {encoding: form, unknown: ignore, query: [input.dry_run, input.note]}
    outcomes:
      recounted: {status: 200}
";

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"query-beside-body\"\n",
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

/// [`ROUTES`] compiled by ESS against `ir`.
fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = format!(
        "type: ess-http/1
specification:
  system: depot
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

/// The depot served by a host through [`ROUTES`], and read by ESS's interpreter.
struct Depot {
    surface: Surface,
    host: Host,
    interpreter: Interpreted,
    _spec: tempfile::TempDir,
    _state: tempfile::TempDir,
}

impl Depot {
    fn open() -> Self {
        let spec = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a spec directory");
        std::fs::create_dir(spec.path().join("domains")).expect("the domains directory");
        std::fs::write(spec.path().join("system.yaml"), SYSTEM).expect("the system");
        std::fs::write(spec.path().join("domains/stock.yaml"), DOMAIN).expect("the domain");
        let ir = spindle_spec::load_model(spec.path())
            .unwrap_or_else(|error| panic!("the depot compiles: {error:?}"))
            .ir;
        let bound = binding(&ir);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&bound))
            .unwrap_or_else(|refused| panic!("the depot surface is routed: {refused}"));
        let unserved: Vec<String> = surface
            .unserved()
            .iter()
            .filter_map(|route| route.not_served())
            .collect();
        assert!(
            unserved.is_empty(),
            "every depot route is served: {unserved:?}"
        );
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(
            ir.clone(),
            &in_memory(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("the host admits the depot: {error}"));
        Self {
            surface,
            host,
            interpreter: Interpreted::for_model(ir),
            _spec: spec,
            _state: state,
        }
    }

    fn post(&mut self, target: &str, content_type: &str, body: &str) -> Response {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: "POST".to_owned(),
                path: path.to_owned(),
                query: query.to_owned(),
                content_type: Some(content_type.to_owned()),
                body: body.as_bytes().to_vec(),
            },
        )
        .response
    }

    /// What ESS's interpreter publishes for `command` run on `input`: `(outcome, payloads)`.
    fn interpreted(&self, command: &str, input: Value) -> (String, Vec<Value>) {
        let input: BTreeMap<String, Node> =
            serde_json::from_value(input).expect("the input as ESS values");
        let result = self
            .interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input,
                correlation: CorrelationId::new("query-beside-body").expect("a correlation"),
            })
            .unwrap_or_else(|error| panic!("the interpreter runs {command}: {error:?}"));
        let outcome = result
            .outcome
            .map(|outcome| outcome.to_string())
            .unwrap_or_default();
        let payloads = result
            .direct_events
            .iter()
            .map(|event| serde_json::to_value(&event.payload).expect("a payload as JSON"))
            .collect();
        (outcome, payloads)
    }
}

fn parsed(response: &Response) -> Value {
    serde_json::from_str(&response.body)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", response.body))
}

/// `response` ran `command` on `joined`, the body's fields with the query's: it answers what ESS's
/// interpreter publishes for that input.
fn ran_as_interpreted(depot: &Depot, response: &Response, command: &str, joined: Value) {
    assert_eq!(response.status, 200, "{}", response.body);
    let body = parsed(response);
    let (outcome, payloads) = depot.interpreted(command, joined.clone());
    assert!(
        outcome.ends_with(body["outcome"].as_str().unwrap_or("none")),
        "the interpreter selects {outcome}: {body}"
    );
    let published: Vec<Value> = body["published"]
        .as_array()
        .unwrap_or_else(|| panic!("published events: {body}"))
        .iter()
        .map(|event| event["payload"].clone())
        .collect();
    assert_eq!(published, payloads, "{command} on {joined}: {body}");
    assert_eq!(published, [joined], "{body}");
}

/// `response` is Spindle's `400` `{"refused": ...}`, its text holding `detail`.
fn refused(response: &Response, detail: &str) {
    assert_eq!(response.status, 400, "{}", response.body);
    let refused = parsed(response)["refused"].clone();
    assert!(
        refused.as_str().is_some_and(|text| text.contains(detail)),
        "{detail:?} in {refused}"
    );
}

#[test]
fn a_command_reads_its_query_fields_beside_the_body() {
    let mut depot = Depot::open();
    let restocked = depot.post("/restock?dryRun=true", JSON, r#"{"batch_id":"b1"}"#);
    ran_as_interpreted(
        &depot,
        &restocked,
        RESTOCK,
        json!({"batch_id": "b1", "dry_run": true, "tags": []}),
    );

    let recounted = depot.post(
        "/recount?dryRun=false&origin=web",
        FORM,
        "batch_id=b2&tags=x",
    );
    ran_as_interpreted(
        &depot,
        &recounted,
        RECOUNT,
        json!({"batch_id": "b2", "dry_run": false, "tags": ["x"]}),
    );

    let noted = depot.post("/recount?note=hi%20there&dryRun=true", FORM, "batch_id=b3");
    ran_as_interpreted(
        &depot,
        &noted,
        RECOUNT,
        json!({"batch_id": "b3", "dry_run": true, "tags": [], "note": "hi there"}),
    );
}

#[test]
fn a_missing_required_query_field_is_refused_400() {
    let mut depot = Depot::open();
    for (target, content_type, body) in [
        ("/restock", JSON, r#"{"batch_id":"b1"}"#),
        ("/recount?note=n", FORM, "batch_id=b1"),
    ] {
        let response = depot.post(target, content_type, body);
        refused(&response, "dry_run");
    }
}

#[test]
fn a_body_member_the_query_carries_is_refused_400() {
    let mut depot = Depot::open();
    for (target, content_type, body, member) in [
        (
            "/restock",
            JSON,
            r#"{"batch_id":"b1","dryRun":true}"#,
            "dryRun",
        ),
        (
            "/restock?dryRun=true",
            JSON,
            r#"{"batch_id":"b1","dryRun":true}"#,
            "dryRun",
        ),
        (
            "/restock?dryRun=true",
            JSON,
            r#"{"batch_id":"b1","dry_run":true}"#,
            "dry_run",
        ),
        (
            "/restock?dryRun=true",
            JSON,
            r#"{"batch_id":"b1","tags":["a"]}"#,
            "tags",
        ),
        ("/recount", FORM, "batch_id=b1&dryRun=true", "dryRun"),
        (
            "/recount?dryRun=true",
            FORM,
            "batch_id=b1&dryRun=true",
            "dryRun",
        ),
        ("/recount?dryRun=true", FORM, "batch_id=b1&note=n", "note"),
    ] {
        let response = depot.post(target, content_type, body);
        refused(&response, &format!("body.{member}{CARRIED}"));
    }
}

#[test]
fn a_list_query_field_reads_each_repeated_key() {
    let mut depot = Depot::open();
    let restocked = depot.post(
        "/restock?tags=a&dryRun=false&tags=b",
        JSON,
        r#"{"batch_id":"b1","note":"n"}"#,
    );
    ran_as_interpreted(
        &depot,
        &restocked,
        RESTOCK,
        json!({"batch_id": "b1", "dry_run": false, "tags": ["a", "b"], "note": "n"}),
    );
    let one = depot.post(
        "/restock?dryRun=true&tags=a,b",
        JSON,
        r#"{"batch_id":"b2"}"#,
    );
    ran_as_interpreted(
        &depot,
        &one,
        RESTOCK,
        json!({"batch_id": "b2", "dry_run": true, "tags": ["a,b"]}),
    );
}

#[test]
fn a_query_key_out_of_its_rules_is_refused_400_as_ess_reads_it() {
    let mut depot = Depot::open();
    for (target, detail) in [
        (
            "/restock?dryRun=true&origin=web",
            "query.origin: no field of this request is named so",
        ),
        (
            "/restock?dry_run=true",
            "query.dry_run: no field of this request is named so",
        ),
        (
            "/restock?dryRun=true&dryRun=false",
            "query.dryRun: expected one value, found 2",
        ),
        (
            "/restock?dryRun=true&batch_id=b9",
            "query.batch_id: no field of this request is named so",
        ),
    ] {
        let response = depot.post(target, JSON, r#"{"batch_id":"b1"}"#);
        refused(&response, detail);
    }
    let twice = depot.post("/recount?dryRun=true&dryRun=false", FORM, "batch_id=b1");
    refused(&twice, "query.dryRun: expected one value, found 2");
}
