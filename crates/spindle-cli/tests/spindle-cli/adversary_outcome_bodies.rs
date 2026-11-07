//! Adversary cases for `story:http-outcome-bodies`: a command outcome an `ess-http/1` binding
//! answers with `body: response`, `body: response.<field>` or `body: none`.
//!
//! The authority is what ESS 0.88.0 publishes for the binding (`ess_gen::openapi::bound_json`,
//! which `ess generate --kind openapi --http` writes) and the server ESS synthesises for it
//! (`ess-synth`, `rust/http/bound.rs`, `outcome_arm`): `response` is `http::bare(response)` and
//! `response.<field>` is `response.member(<wire name>)`, both over the envelope's `response`,
//! whose members are the fields' wire names; `none` is `http::Response::empty(status)`.
//!
//! `tests/fixtures/adversary-outcome-bodies` declares one component reached over the network.
//! `tests/fixtures/adversary-outcome-bodies-http.yaml` binds `OpenLocker` to `body: response`
//! over a response with a field renamed on the wire (`receipt_code` as `receiptCode`),
//! `RentLocker` to `body: response.receipt_code`, `TicketLocker` to `body: response.ticket`, a
//! struct with a member renamed on the wire (`ticket_code` as `ticketCode`), `NumberLocker` to
//! `body: response.slot`, an `Integer`, `CloseLocker` to `200` with `body: none` and its two
//! refusals (`alike`) to `409` with `body: none`, and `PriceLocker`'s `priced`, whose response is
//! a struct with an invariant the runtime does not mint, to `body: response`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const FIXTURE: &str = "crates/spindle-cli/tests/fixtures/adversary-outcome-bodies";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/adversary-outcome-bodies-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "vault";

const OPEN: &str = "vault.locker.OpenLocker";
const RENT: &str = "vault.locker.RentLocker";
const TICKET: &str = "vault.locker.TicketLocker";
const NUMBER: &str = "vault.locker.NumberLocker";
const CLOSE: &str = "vault.locker.CloseLocker";
const PRICE: &str = "vault.locker.PriceLocker";
const OPEN_LOCKERS: &str = "vault.locker.OpenLockers";

const SEED: u64 = 11;
const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn binding(ir: &ess_compiler::EssIr) -> ess_gen::http::Binding {
    let text = std::fs::read_to_string(root().join(BINDING)).expect("the binding is read");
    let parsed = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&parsed, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http)
        .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"))
}

/// The `OpenAPI` document ESS projects for the fixture's component, with the binding where asked.
struct Contract(Value);

impl Contract {
    fn load(bound: bool) -> Self {
        let loaded = spindle_spec::load(&root().join(FIXTURE)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let binding = bound.then(|| binding(&loaded.ir));
        let document = ess_gen::openapi::bound_json(&loaded.ir, component, binding.as_ref());
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    fn route(&self, id: &str) -> (String, String) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone());
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    fn response(&self, id: &str, status: u16) -> &Value {
        let (method, path) = self.route(id);
        let response =
            &self.0["paths"][&path][method.to_lowercase()]["responses"][status.to_string()];
        assert!(
            !response.is_null(),
            "the contract declares no answer of {id} under {status}"
        );
        response
    }

    /// The errors the JSON schema the contract declares for `id` under `status` finds in `body`.
    fn errors(&self, id: &str, status: u16, body: &Value) -> Vec<String> {
        let schema = &self.response(id, status)["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect()
    }
}

struct Served {
    child: Child,
    address: SocketAddr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = state.path().join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", FIXTURE, "--environment", LAPTOP, "--state"])
        .arg(&file)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = match receiver.recv_timeout(STARTUP) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run printed no line within {STARTUP:?}: {error}");
        }
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} is not JSON ({error})")
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    let served = Served {
        child,
        address,
        _state: state,
    };
    let reset = served.exchange("POST", "/api/reset", Some(&json!({"mint": SEED})));
    assert_eq!(reset.status, 200, "{reset:?}");
    served
}

#[derive(Debug)]
struct Reply {
    status: u16,
    head: Vec<String>,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.head.iter().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then_some(value.trim())
        })
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|error| panic!("{error}: the body is not JSON: {self:?}"))
    }
}

impl Served {
    fn exchange(&self, method: &str, path: &str, body: Option<&Value>) -> Reply {
        let mut stream =
            TcpStream::connect_timeout(&self.address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(ANSWER))
            .expect("a read timeout");
        let body = body.map(Value::to_string).unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            self.address,
            body.len()
        )
        .expect("the request is written");
        let mut answer = Vec::new();
        stream
            .read_to_end(&mut answer)
            .expect("the answer is read before the timeout");
        let answer = String::from_utf8(answer).expect("the answer is UTF-8");
        let (head, body) = answer
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("a status line: {head:?}"));
        Reply {
            status,
            head: lines.map(str::to_owned).collect(),
            body: body.to_owned(),
        }
    }

    fn send(&self, contract: &Contract, id: &str, input: &Value) -> Reply {
        let (method, path) = contract.route(id);
        self.exchange(&method, &path, Some(input))
    }

    fn rows(&self, contract: &Contract) -> Value {
        let (method, path) = contract.route(OPEN_LOCKERS);
        let reply = self.exchange(&method, &path, None);
        assert_eq!(reply.status, 200, "{reply:?}");
        reply.json()
    }
}

#[test]
fn a_response_body_keys_a_renamed_response_field_by_its_wire_name() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);
    let reply = served.send(
        &contract,
        OPEN,
        &json!({"locker_id": "l1", "label": "coat"}),
    );
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    let body = reply.json();
    assert!(
        body.get("receiptCode").is_some_and(Value::is_string) && body.get("receipt_code").is_none(),
        "story:http-outcome-bodies: `body: response` is the response as ESS publishes it and its \
         served application answers it (`http::bare(response)`), `receipt_code` under its wire \
         name `receiptCode`: {body}"
    );
    let errors = contract.errors(OPEN, reply.status, &body);
    assert!(
        errors.is_empty(),
        "story:http-outcome-bodies: {body} is refused by the schema ESS projects for {OPEN} 201: \
         {errors:?}"
    );
}

#[test]
fn a_response_field_body_of_a_renamed_field_is_its_value() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);
    let reply = served.send(&contract, RENT, &json!({"locker_id": "l2", "label": "hat"}));
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    let body = reply.json();
    assert!(
        body.as_str().is_some_and(|text| !text.is_empty()),
        "`response.receipt_code` names the field by its local name and answers its value, never \
         null: {body}"
    );
    let errors = contract.errors(RENT, reply.status, &body);
    assert!(errors.is_empty(), "{body}: {errors:?}");
}

#[test]
fn a_response_field_body_of_a_struct_keys_its_members_by_wire_name() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);
    let reply = served.send(
        &contract,
        TICKET,
        &json!({"locker_id": "l3", "label": "bag"}),
    );
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    let body = reply.json();
    assert!(
        body.get("ticketCode").is_some_and(Value::is_string) && body.get("ticket_code").is_none(),
        "story:http-outcome-bodies: `response.ticket` is the struct as ESS publishes it, its \
         member `ticket_code` under its wire name `ticketCode`: {body}"
    );
    let errors = contract.errors(TICKET, reply.status, &body);
    assert!(
        errors.is_empty(),
        "story:http-outcome-bodies: {body} is refused by the schema ESS projects for {TICKET} \
         201: {errors:?}"
    );
}

#[test]
fn an_integer_response_field_body_is_a_bare_json_number() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);
    let reply = served.send(
        &contract,
        NUMBER,
        &json!({"locker_id": "l4", "label": "box"}),
    );
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    let body = reply.json();
    assert!(body.is_i64(), "the integer alone: {body}");
    let errors = contract.errors(NUMBER, reply.status, &body);
    assert!(errors.is_empty(), "{body}: {errors:?}");
}

#[test]
fn a_none_body_on_200_is_empty_with_zero_length_and_no_content_type() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);
    let opened = served.send(
        &contract,
        NUMBER,
        &json!({"locker_id": "l5", "label": "lamp"}),
    );
    assert_eq!(opened.status, 201, "{opened:?}");
    let reply = served.send(&contract, CLOSE, &json!({"locker_id": "l5"}));
    assert_eq!(reply.status, 200, "the bound status: {reply:?}");
    assert_eq!(reply.body, "", "no body: {reply:?}");
    assert_eq!(reply.header("content-type"), None, "{reply:?}");
    assert_eq!(reply.header("content-length"), Some("0"), "{reply:?}");
    assert!(
        contract.response(CLOSE, 200).get("content").is_none(),
        "the contract declares no content under 200: {}",
        contract.response(CLOSE, 200)
    );
    assert_eq!(served.rows(&contract), json!({"rows": []}), "the close ran");
}

#[test]
fn a_none_body_on_refusals_answers_their_bound_status_empty_and_writes_nothing() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);

    let unknown = served.send(&contract, CLOSE, &json!({"locker_id": "nobody"}));
    assert_eq!(unknown.status, 409, "the bound status: {unknown:?}");
    assert_eq!(unknown.body, "", "no body: {unknown:?}");
    assert_eq!(unknown.header("content-type"), None, "{unknown:?}");
    assert_eq!(unknown.header("content-length"), Some("0"), "{unknown:?}");

    let opened = served.send(
        &contract,
        NUMBER,
        &json!({"locker_id": "l6", "label": "mug"}),
    );
    assert_eq!(opened.status, 201, "{opened:?}");
    let closed = served.send(&contract, CLOSE, &json!({"locker_id": "l6"}));
    assert_eq!(closed.status, 200, "{closed:?}");
    let again = served.send(&contract, CLOSE, &json!({"locker_id": "l6"}));
    assert_eq!(again.status, 409, "the bound status: {again:?}");
    assert_eq!(again.body, "", "no body: {again:?}");
    assert_eq!(again.header("content-type"), None, "{again:?}");
    assert!(
        contract.response(CLOSE, 409).get("content").is_none(),
        "the contract declares no content under 409: {}",
        contract.response(CLOSE, 409)
    );
    assert_eq!(served.rows(&contract), json!({"rows": []}));
}

#[test]
fn an_outcome_bound_with_a_body_the_kernel_cannot_run_answers_501_and_writes_nothing() {
    let contract = Contract::load(true);
    let served = serve(&["--http", BINDING]);
    let reply = served.send(
        &contract,
        PRICE,
        &json!({"locker_id": "l7", "label": "pen", "price": true}),
    );
    assert_eq!(reply.status, 501, "{reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    let answer = reply.json();
    assert_eq!(answer["committed"], false, "{answer}");
    let refused = answer["refused"].as_str().expect("a refusal in words");
    assert!(
        refused.contains("vault.locker.PriceLocker/priced"),
        "the 501 names the outcome: {refused}"
    );
    assert_eq!(
        served.rows(&contract),
        json!({"rows": []}),
        "nothing written"
    );

    let noted = served.send(
        &contract,
        PRICE,
        &json!({"locker_id": "l8", "label": "ink", "price": false}),
    );
    assert!((200..300).contains(&noted.status), "{noted:?}");
    let envelope = noted.json();
    assert_eq!(
        envelope["outcome"], "noted",
        "the other outcome keeps the envelope: {envelope}"
    );
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"locker_id": "l8", "label": "ink"}]})
    );
}

#[test]
fn the_router_answers_a_none_body_empty() {
    let loaded = spindle_spec::load(&root().join(FIXTURE)).expect("the fixture compiles");
    let ir = loaded.ir;
    let binding = binding(&ir);
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], Some(&binding))
        .unwrap_or_else(|refused| panic!("the surface is served: {refused}"));
    let environment = spindle_capability::Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-outcome-bodies\"\n",
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
    .expect("the environment parses");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut host = spindle_host::Host::new(
        ir,
        &environment,
        Arc::new(spindle_host::ManualClock::new()),
        &state.path().join("state"),
    )
    .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"));
    let request = |path: &str, body: Value| spindle_ingress::Request {
        method: "POST".to_owned(),
        path: path.to_owned(),
        query: String::new(),
        content_type: Some("application/json".to_owned()),
        body: body.to_string().into_bytes(),
    };

    let unknown = spindle_ingress::route(
        &surface,
        &mut host,
        request("/lockers/closed", json!({"locker_id": "nobody"})),
    );
    assert_eq!(unknown.response.status, 409, "{:?}", unknown.response);
    assert_eq!(unknown.response.body, "", "{:?}", unknown.response);

    let opened = spindle_ingress::route(
        &surface,
        &mut host,
        request(
            "/lockers/numbered",
            json!({"locker_id": "l9", "label": "cup"}),
        ),
    );
    assert_eq!(opened.response.status, 201, "{:?}", opened.response);
    let slot: Value = serde_json::from_str(&opened.response.body).expect("a JSON body");
    assert!(slot.is_i64(), "{slot}");

    let closed = spindle_ingress::route(
        &surface,
        &mut host,
        request("/lockers/closed", json!({"locker_id": "l9"})),
    );
    assert_eq!(closed.response.status, 200, "{:?}", closed.response);
    assert_eq!(closed.response.body, "", "{:?}", closed.response);
}

#[test]
fn the_envelope_response_keys_a_renamed_field_by_its_wire_name() {
    let contract = Contract::load(false);
    let served = serve(&[]);
    let reply = served.send(
        &contract,
        OPEN,
        &json!({"locker_id": "l10", "label": "key"}),
    );
    assert!((200..300).contains(&reply.status), "{reply:?}");
    let envelope = reply.json();
    let response = &envelope["response"];
    assert!(
        response.get("receiptCode").is_some_and(Value::is_string)
            && response.get("receipt_code").is_none(),
        "the envelope's `response` carries `receipt_code` under its wire name `receiptCode`, as \
         ESS publishes it: {envelope}"
    );
    let errors = contract.errors(OPEN, reply.status, &envelope);
    assert!(
        errors.is_empty(),
        "{envelope} is refused by the schema ESS projects for {OPEN}: {errors:?}"
    );
}
