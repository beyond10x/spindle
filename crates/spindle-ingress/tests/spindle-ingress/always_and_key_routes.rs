//! The routes of views whose filter holds an `(always)` leaf inside a quantifier or reads a
//! member of the row's struct identity (`key.integration == param.id`), served over HTTP at the
//! routes ESS derives and answering the rows ESS's interpreter answers, where they answered 501
//! (`story:view-filter-always-and-key`).
//!
//! The fixture is `spindle-host`'s `tests/fixtures/always-and-key`: one component, `links`,
//! reached over the network, keeping links keyed by an integration and a name, each with tags.
//! Every route is read from the `OpenAPI` document ESS projects for the component, and every
//! answer is validated against the schema it declares.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "links";
const ADD: &str = "desk.links.AddLink";
const TAGGED: &str = "desk.links.Tagged";
const OF_INTEGRATION: &str = "desk.links.OfIntegration";
const OF_INTEGRATION_TAGGED: &str = "desk.links.OfIntegrationTagged";
const NAMED: &str = "desk.links.Named";
const LISTED: &str = "desk.links.Listed";
const COMPARED: &str = "desk.links.Compared";
const REGIONAL: &str = "desk.links.Regional";

/// Every read: a view, its query string, and the same values as ESS parameters.
const READS: [(&str, &str, &str); 12] = [
    (TAGGED, "", "{}"),
    (TAGGED, "tags=red", r#"{"tags": ["red"]}"#),
    (
        TAGGED,
        "tags=blue&tags=green",
        r#"{"tags": ["blue", "green"]}"#,
    ),
    (OF_INTEGRATION, "id=mail", r#"{"id": "mail"}"#),
    (OF_INTEGRATION, "id=none", r#"{"id": "none"}"#),
    (OF_INTEGRATION_TAGGED, "id=chat", r#"{"id": "chat"}"#),
    (
        OF_INTEGRATION_TAGGED,
        "id=chat&tags=red",
        r#"{"id": "chat", "tags": ["red"]}"#,
    ),
    (
        OF_INTEGRATION_TAGGED,
        "id=mail&tags=blue",
        r#"{"id": "mail", "tags": ["blue"]}"#,
    ),
    (NAMED, "", "{}"),
    (LISTED, "", "{}"),
    (COMPARED, "", "{}"),
    (REGIONAL, "", "{}"),
];

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn ir() -> EssIr {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-host/tests/fixtures/always-and-key");
    spindle_spec::load(&fixture)
        .expect("the always-and-key fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"always-and-key\"\n",
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

/// The links added before every read. `mail/beta` holds no tag.
fn links() -> Vec<Value> {
    vec![
        json!({"key": {"integration": "mail", "name": "alpha"}, "label": "one", "origin": {"place": {"region": "eu"}}, "tags": ["red"]}),
        json!({"key": {"integration": "mail", "name": "beta"}, "label": "two", "origin": {"place": {"region": "us"}}, "tags": []}),
        json!({"key": {"integration": "chat", "name": "alpha"}, "label": "three", "origin": {"place": {"region": "eu"}},
               "tags": ["red", "blue"]}),
        json!({"key": {"integration": "chat", "name": "gamma"}, "label": "gamma", "origin": {"place": {"region": "us"}},
               "tags": ["blue"]}),
    ]
}

/// The `OpenAPI` document ESS projects for `links`.
fn contract(ir: &EssIr) -> Value {
    let component = ir
        .components()
        .values()
        .find(|component| component.name.as_str() == COMPONENT)
        .expect("the fixture declares the component");
    serde_json::from_str(&ess_gen::openapi::json(ir, component)).expect("the projection is JSON")
}

/// The method and path of `id` in `contract`.
fn operation(contract: &Value, id: &str) -> (String, String) {
    for (path, item) in contract["paths"]
        .as_object()
        .expect("the document has paths")
    {
        for (method, operation) in item.as_object().expect("a path item is an object") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

/// Fails unless `body` validates against the schema `contract` declares for `id`'s `200`.
fn validate(contract: &Value, id: &str, body: &Value) {
    let (method, path) = operation(contract, id);
    let schema = &contract["paths"][&path][method.to_lowercase()]["responses"]["200"]["content"]["application/json"]
        ["schema"];
    assert!(!schema.is_null(), "{id} declares no JSON 200 answer");
    let mut document = contract.clone();
    document["allOf"] = json!([schema]);
    let validator =
        jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
    let errors = validator
        .iter_errors(body)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{id} answered {body}: {errors:?}");
}

/// One HTTP/1.1 request with no body, `Connection: close`: the status and the JSON body.
fn request(address: SocketAddr, method: &str, target: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
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
    let body = serde_json::from_str(body).unwrap_or_else(|_| panic!("not JSON: {body}"));
    (status, body)
}

/// Each route of the fixture's views answers `200` with the rows ESS's interpreter answers.
#[test]
fn always_and_key_routes_answer_the_interpreters_rows() {
    let ir = ir();
    let contract = contract(&ir);
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .unwrap_or_else(|refused| panic!("the links surface is served: {refused}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let interpreter = Interpreted::for_model(ir.clone());
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the links: {error}"));
    let correlation = CorrelationId::new("always-and-key").expect("a correlation");
    for input in links() {
        let fields = match input.clone() {
            Value::Object(fields) => fields,
            other => panic!("not an object: {other}"),
        };
        host.submit(ADD, fields)
            .unwrap_or_else(|error| panic!("{ADD} runs on the host: {error}"));
        interpreter
            .execute_command(SemanticCommandRequest {
                command: ADD.parse().expect("a command name"),
                actor: None,
                caller: None,
                input: serde_json::from_value(input).expect("the input as ESS values"),
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{ADD} runs on the interpreter: {error:?}"));
    }
    let address = listener.local_addr();
    for (view, query, params) in READS {
        let (method, path) = operation(&contract, view);
        let target = if query.is_empty() {
            path
        } else {
            format!("{path}?{query}")
        };
        let client = thread::spawn(move || request(address, &method, &target));
        for _ in 0..STEPS {
            if client.is_finished() {
                break;
            }
            if let Some(exchange) = listener.next(WAIT) {
                exchange.serve(&mut host);
            }
        }
        assert!(
            client.is_finished(),
            "{view}?{query} was not answered in time"
        );
        let (status, body) = client.join().expect("the client thread");
        assert_eq!(status, 200, "{view}?{query}: {body}");
        validate(&contract, view, &body);
        let params: BTreeMap<String, Node> =
            serde_json::from_str(params).expect("the parameters as ESS values");
        let answer = interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("the interpreter reads {view}: {error:?}"));
        let expected = Map::from_iter([(
            "rows".to_owned(),
            serde_json::to_value(answer.rows).expect("rows as JSON"),
        )]);
        assert_eq!(
            body,
            Value::Object(expected),
            "{view}?{query}: the rows ESS's interpreter answers"
        );
    }
}
