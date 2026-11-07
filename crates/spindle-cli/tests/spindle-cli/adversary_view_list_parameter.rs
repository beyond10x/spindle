//! Adversary cases for `story:view-list-query-parameters`: a view's `List<T>` parameter read from
//! its repeated query key, attacked with encoded keys and values, every scalar element kind, a
//! long list, `forall`, a shadowing and a nested binder, an unservable element and the shape of a
//! refusal. Every set of rows is compared with the rows ESS's interpreter answers for the same
//! parameter values over the same crates.
//!
//! `tests/fixtures/adversary-view-list-parameter` declares one component, `stack`, reached over
//! the network: `FileCrate` files a crate with a tag, a label, a number, an amount, a seal, a
//! colour and an instant, and each view selects crates by a list of one of them.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-view-list-parameter";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "stack";

const FILE_CRATE: &str = "yard.stack.FileCrate";
const BY_TAGS: &str = "yard.stack.ByTags";
const BY_NUMBERS: &str = "yard.stack.ByNumbers";
const BY_IDS: &str = "yard.stack.ByIds";
const BY_SEAL: &str = "yard.stack.BySeal";
const BY_AMOUNTS: &str = "yard.stack.ByAmounts";
const BY_COLOURS: &str = "yard.stack.ByColours";
const PACKED_AT_ANY: &str = "yard.stack.PackedAtAny";
const PACKED_AT: &str = "yard.stack.PackedAt";
const EXCLUDING: &str = "yard.stack.Excluding";
const MAYBE_EXCLUDING: &str = "yard.stack.MaybeExcluding";
const SHADOWED: &str = "yard.stack.Shadowed";
const PAIRED: &str = "yard.stack.Paired";
const AT_SPOTS: &str = "yard.stack.AtSpots";
const WITH_TAG: &str = "yard.stack.WithTag";
const WITH_NUMBER: &str = "yard.stack.WithNumber";
const WITH_AMOUNT: &str = "yard.stack.WithAmount";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The crates every test files.
fn crates() -> Vec<Value> {
    vec![
        json!({"crate_id": "C-1", "tag": "a", "label": "x", "number": 1, "amount": 1.5,
               "sealed": true, "colour": "Red", "packed_at": "2026-01-01T00:00:00Z"}),
        json!({"crate_id": "C-2", "tag": "b", "label": "a", "number": 2, "amount": 2,
               "sealed": false, "colour": "Blue", "packed_at": "2026-01-02T00:00:00Z"}),
        json!({"crate_id": "C-3", "tag": "a b", "label": "b", "number": 3, "amount": 3.25,
               "sealed": true, "colour": "Blue", "packed_at": "2026-01-03T00:00:00Z"}),
        json!({"crate_id": "C-4", "tag": "+", "label": "+", "number": 1, "amount": 0.5,
               "sealed": false, "colour": "Red", "packed_at": "2026-01-04T12:00:00Z"}),
    ]
}

/// The `OpenAPI` document ESS projects for `stack`.
struct Contract(Value);

impl Contract {
    fn of(ir: &ess_compiler::EssIr) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::json(ir, component);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    fn operation(&self, id: &str) -> (String, String) {
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

fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
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
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        _state: state,
    }
}

fn exchange(address: SocketAddr, method: &str, target: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"))
    };
    (status, body)
}

struct Yard {
    served: Served,
    contract: Contract,
    interpreter: Interpreted,
}

impl Yard {
    fn open() -> Self {
        let ir = spindle_spec::load(&root().join(SPEC))
            .expect("the fixture compiles")
            .ir;
        let contract = Contract::of(&ir);
        let interpreter = Interpreted::for_model(ir);
        let served = serve();
        let correlation =
            CorrelationId::new("adversary-view-list-parameter").expect("a correlation");
        let (method, path) = contract.operation(FILE_CRATE);
        for input in crates() {
            let (status, answer) = exchange(served.address, &method, &path, Some(&input));
            assert_eq!(status, 202, "{FILE_CRATE} {input}: {answer}");
            let input: BTreeMap<String, Node> =
                serde_json::from_value(input).expect("the input as ESS values");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: FILE_CRATE.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{FILE_CRATE} runs on the interpreter: {error:?}"));
        }
        Self {
            served,
            contract,
            interpreter,
        }
    }

    fn read(&self, view: &str, query: &str) -> (u16, Value) {
        let (method, path) = self.contract.operation(view);
        let target = if query.is_empty() {
            path
        } else {
            format!("{path}?{query}")
        };
        exchange(self.served.address, &method, &target, None)
    }

    /// The rows ESS's interpreter answers for `view` read with `params`, or `None` where it
    /// refuses the read.
    fn interpreted(&self, view: &str, params: Value) -> Option<Value> {
        let params: BTreeMap<String, Node> =
            serde_json::from_value(params).expect("the parameters as ESS values");
        self.interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("adversary-view-list-parameter")
                    .expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .ok()
            .map(|answer| json!({"rows": serde_json::to_value(answer.rows).expect("rows")}))
    }

    /// Fails unless `view?query` answers `200` with exactly the rows the interpreter answers for
    /// `params`, which must be `ids`.
    fn agrees(&self, view: &str, query: &str, params: Value, ids: &[&str]) {
        let expected = self
            .interpreted(view, params.clone())
            .unwrap_or_else(|| panic!("the interpreter reads {view} with {params}"));
        let wanted =
            json!({"rows": ids.iter().map(|id| json!({"crate_id": id})).collect::<Vec<_>>()});
        assert_eq!(
            expected, wanted,
            "the interpreter's rows for {view} {params}"
        );
        let (status, body) = self.read(view, query);
        assert_eq!(
            (status, &body),
            (200, &expected),
            "{view}?{query} answers the interpreter's rows for {params}"
        );
    }
}

/// Fails unless `answer` is a `400` refusal naming the query parameter `wire`.
fn refused_naming(status: u16, answer: &Value, wire: &str, asked: &str) {
    assert_eq!(status, 400, "{asked}: {answer}");
    let refused = answer["refused"]
        .as_str()
        .unwrap_or_else(|| panic!("{asked}: a refusal in words: {answer}"));
    assert!(
        refused.contains(&format!("`{wire}`")) || refused.contains(&format!("query.{wire}")),
        "{asked}: the refusal names the parameter {wire}: {refused}"
    );
}

#[test]
fn encoded_keys_and_values_are_read_as_ess_decodes_them() {
    let yard = Yard::open();
    yard.agrees(BY_TAGS, "t%61gs=a", json!({"tags": ["a"]}), &["C-1"]);
    yard.agrees(BY_TAGS, "tags=a+b", json!({"tags": ["a b"]}), &["C-3"]);
    yard.agrees(
        BY_TAGS,
        "tags=a%20b&tags=%2B",
        json!({"tags": ["a b", "+"]}),
        &["C-3", "C-4"],
    );
    yard.agrees(BY_TAGS, "&&tags=b&&", json!({"tags": ["b"]}), &["C-2"]);
}

#[test]
fn a_bracketed_or_recased_key_is_not_the_parameter() {
    let yard = Yard::open();
    for query in ["tags%5B%5D=a", "tags[]=a", "TAGS=a", "Tags=a"] {
        let (status, answer) = yard.read(BY_TAGS, query);
        refused_naming(status, &answer, "tags", query);
    }
}

#[test]
fn a_scalar_key_repeated_is_still_refused_400() {
    let yard = Yard::open();
    let (status, answer) = yard.read(WITH_TAG, "tag=a&tag=b");
    refused_naming(status, &answer, "tag", "tag=a&tag=b");
    let (status, answer) = yard.read(WITH_NUMBER, "number=1&number=1");
    refused_naming(status, &answer, "number", "number=1&number=1");
}

#[test]
fn every_scalar_element_kind_answers_the_interpreters_rows() {
    let yard = Yard::open();
    yard.agrees(
        BY_IDS,
        "ids=C-2&ids=C-4",
        json!({"ids": ["C-2", "C-4"]}),
        &["C-2", "C-4"],
    );
    yard.agrees(
        BY_SEAL,
        "seals=false",
        json!({"seals": [false]}),
        &["C-2", "C-4"],
    );
    yard.agrees(
        BY_SEAL,
        "seals=true&seals=false",
        json!({"seals": [true, false]}),
        &["C-1", "C-2", "C-3", "C-4"],
    );
    yard.agrees(
        BY_COLOURS,
        "colours=Blue",
        json!({"colours": ["Blue"]}),
        &["C-2", "C-3"],
    );
    yard.agrees(
        BY_NUMBERS,
        "numbers=-0&numbers=1",
        json!({"numbers": [0, 1]}),
        &["C-1", "C-4"],
    );
}

/// ESS publishes a `Decimal` as a decimal string (`ess-gen` `types.rs:661`, `format: decimal`),
/// and its served application reads a query value of one as text (`view_query::query_scalar`).
/// The element `3.25` is a valid value of the published schema, and the interpreter selects its
/// rows.
#[test]
fn a_decimal_element_answers_the_interpreters_rows() {
    let yard = Yard::open();
    yard.agrees(
        BY_AMOUNTS,
        "amounts=3.25&amounts=2",
        json!({"amounts": [3.25, 2]}),
        &["C-2", "C-3"],
    );
}

/// Control for [`a_decimal_element_answers_the_interpreters_rows`]: the scalar path, which this
/// story did not change, reads a `Decimal` the same way.
#[test]
fn a_scalar_decimal_answers_the_interpreters_rows() {
    let yard = Yard::open();
    yard.agrees(
        WITH_AMOUNT,
        "amount=3.25",
        json!({"amount": 3.25}),
        &["C-3"],
    );
}

#[test]
fn an_instant_element_selects_the_rows_of_its_instant_however_spelt() {
    let yard = Yard::open();
    yard.agrees(
        PACKED_AT_ANY,
        "ats=2026-01-01T01%3A00%3A00%2B01%3A00",
        json!({"ats": ["2026-01-01T01:00:00+01:00"]}),
        &["C-1"],
    );
}

#[test]
fn a_scalar_instant_selects_the_rows_of_its_instant_however_spelt() {
    let yard = Yard::open();
    yard.agrees(
        PACKED_AT,
        "at=2026-01-01T01%3A00%3A00%2B01%3A00",
        json!({"at": "2026-01-01T01:00:00+01:00"}),
        &["C-1"],
    );
}

#[test]
fn ill_typed_elements_are_refused_400_naming_the_parameter() {
    let yard = Yard::open();
    for (view, wire, query) in [
        (BY_SEAL, "seals", "seals=true&seals=yes"),
        (BY_SEAL, "seals", "seals=TRUE"),
        (BY_COLOURS, "colours", "colours=Green"),
        (BY_IDS, "ids", "ids=X-1"),
        (BY_AMOUNTS, "amounts", "amounts=abc"),
        (BY_NUMBERS, "numbers", "numbers=%2B1"),
        (BY_NUMBERS, "numbers", "numbers=99999999999999999999"),
        (BY_TAGS, "tags", "tags=%ZZ"),
        (BY_TAGS, "tags", "tags"),
    ] {
        let (status, answer) = yard.read(view, query);
        refused_naming(status, &answer, wire, query);
    }
}

#[test]
fn a_long_list_answers_the_interpreters_rows() {
    let yard = Yard::open();
    let mut tags: Vec<String> = (0..3000).map(|index| format!("z{index}")).collect();
    tags.push("b".to_owned());
    let query = tags
        .iter()
        .map(|tag| format!("tags={tag}"))
        .collect::<Vec<_>>()
        .join("&");
    yard.agrees(BY_TAGS, &query, json!({"tags": tags}), &["C-2"]);
}

#[test]
fn duplicate_elements_answer_as_one() {
    let yard = Yard::open();
    yard.agrees(
        BY_TAGS,
        "tags=a&tags=a&tags=a",
        json!({"tags": ["a", "a", "a"]}),
        &["C-1"],
    );
}

#[test]
fn forall_over_a_list_answers_the_interpreters_rows() {
    let yard = Yard::open();
    yard.agrees(
        EXCLUDING,
        "tags=a&tags=b",
        json!({"tags": ["a", "b"]}),
        &["C-3", "C-4"],
    );
    yard.agrees(
        MAYBE_EXCLUDING,
        "tags=%2B",
        json!({"tags": ["+"]}),
        &["C-1", "C-2", "C-3"],
    );
}

#[test]
fn forall_over_an_absent_optional_list_is_refused_as_the_interpreter_refuses() {
    let yard = Yard::open();
    assert_eq!(
        yard.interpreted(MAYBE_EXCLUDING, json!({})),
        None,
        "the interpreter reads no rows for a `forall` over an absent list"
    );
    let (status, body) = yard.read(MAYBE_EXCLUDING, "");
    assert_eq!(status, 501, "{body}");
    let (status, answer) = yard.read(MAYBE_EXCLUDING, "tags=");
    refused_naming(status, &answer, "tags", "tags=");
}

#[test]
fn a_binder_shadowing_a_field_reads_the_element() {
    let yard = Yard::open();
    yard.agrees(SHADOWED, "tags=a", json!({"tags": ["a"]}), &["C-1"]);
}

#[test]
fn nested_binders_answer_the_interpreters_rows() {
    let yard = Yard::open();
    yard.agrees(
        PAIRED,
        "tags=a&numbers=1&tags=%2B",
        json!({"tags": ["a", "+"], "numbers": [1]}),
        &["C-1", "C-4"],
    );
    yard.agrees(
        PAIRED,
        "tags=a&numbers=2",
        json!({"tags": ["a"], "numbers": [2]}),
        &[],
    );
}

#[test]
fn a_struct_element_still_answers_501_naming_the_parameter() {
    let yard = Yard::open();
    let (status, body) = yard.read(AT_SPOTS, "spots=1");
    assert_eq!(status, 501, "{body}");
    assert!(
        body.to_string().contains("spots"),
        "the 501 names spots: {body}"
    );
}

#[test]
fn a_list_refusal_has_the_scalar_refusals_shape() {
    let yard = Yard::open();
    let keys = |body: &Value| {
        body.as_object()
            .map(|members| members.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    for (list, scalar) in [("numbers=", "number="), ("numbers=x", "number=x")] {
        let (list_status, list_body) = yard.read(BY_NUMBERS, list);
        let (scalar_status, scalar_body) = yard.read(WITH_NUMBER, scalar);
        assert_eq!(
            (list_status, keys(&list_body)),
            (scalar_status, keys(&scalar_body)),
            "{list}: {list_body} against {scalar}: {scalar_body}"
        );
    }
}
