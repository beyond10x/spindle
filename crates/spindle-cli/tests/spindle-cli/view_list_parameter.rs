//! `run` serving views whose `List<T>` parameter the query string carries as ESS publishes it:
//! the key repeated once per element (`tags=a&tags=b`, `style: form`, `explode: true`), each
//! element read as `T`, required unless the parameter is `Optional`
//! (`story:view-list-query-parameters`).
//!
//! `tests/fixtures/view-list-parameter` declares one component, `board`, reached over the network:
//! `FileItem` files an item with a `tag` and a `number`; `Tagged` keeps the items whose tag is one
//! of a required `tags: List<String>`, `Numbered` those whose number is one of a required
//! `n: List<Integer>`, and `MaybeTagged` those whose tag is one of an `Optional<List<String>>`
//! `tags`, or every item where it is left out. Every route is read from the `OpenAPI` document ESS
//! projects for the component, every `200` answer is validated against its schema, and every set
//! of rows is compared with the rows ESS's interpreter answers for the same parameter values over
//! the same items.

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

const SPEC: &str = "crates/spindle-cli/tests/fixtures/view-list-parameter";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "board";

const FILE_ITEM: &str = "labels.board.FileItem";
const TAGGED: &str = "labels.board.Tagged";
const NUMBERED: &str = "labels.board.Numbered";
const MAYBE_TAGGED: &str = "labels.board.MaybeTagged";

/// The items every test files, `(item_id, tag, number)`. `i5`'s tag holds a comma, so the comma
/// form of a list (`tags=a,b`) selects it and nothing else.
const ITEMS: [(&str, &str, i64); 5] = [
    ("i1", "a", 1),
    ("i2", "b", 2),
    ("i3", "c", 3),
    ("i4", "a", 4),
    ("i5", "a,b", 5),
];

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for `board`.
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

    /// The method, path and operation object of `id`.
    fn operation(&self, id: &str) -> (String, String, &Value) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone(), operation);
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    /// The query parameter `name` of the view `id`, as the contract declares it.
    fn parameter(&self, id: &str, name: &str) -> &Value {
        let (_, _, operation) = self.operation(id);
        operation["parameters"]
            .as_array()
            .and_then(|parameters| {
                parameters
                    .iter()
                    .find(|parameter| parameter["name"] == name)
            })
            .unwrap_or_else(|| panic!("{id} declares no parameter {name}"))
    }

    /// Fails unless `body` validates against the schema the contract declares for `id`'s answer
    /// under `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path, _) = self.operation(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
}

/// A running `b10x-spindle run` serving the fixture over HTTP; stopped when dropped.
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

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
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
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// `run` serving the fixture with [`ITEMS`] filed, and ESS's interpreter with the same commands
/// run on it.
struct Board {
    served: Served,
    contract: Contract,
    interpreter: Interpreted,
}

impl Board {
    fn open() -> Self {
        let ir = spindle_spec::load(&root().join(SPEC))
            .expect("the fixture compiles")
            .ir;
        let contract = Contract::of(&ir);
        let interpreter = Interpreted::for_model(ir);
        let served = serve();
        let correlation = CorrelationId::new("view-list-parameter").expect("a correlation");
        let (method, path, _) = contract.operation(FILE_ITEM);
        for (id, tag, number) in ITEMS {
            let input = json!({"item_id": id, "tag": tag, "number": number});
            let (status, answer) = exchange(served.address, &method, &path, Some(&input));
            assert_eq!(status, 202, "{FILE_ITEM} {input}: {answer}");
            assert_eq!(answer["outcome"], "filed", "{answer}");
            let input: BTreeMap<String, Node> =
                serde_json::from_value(input).expect("the input as ESS values");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: FILE_ITEM.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{FILE_ITEM} runs on the interpreter: {error:?}"));
        }
        Self {
            served,
            contract,
            interpreter,
        }
    }

    /// `view` read with the query string `query` (none where empty): the status and the body.
    fn read(&self, view: &str, query: &str) -> (u16, Value) {
        let (method, path, _) = self.contract.operation(view);
        let target = if query.is_empty() {
            path
        } else {
            format!("{path}?{query}")
        };
        exchange(self.served.address, &method, &target, None)
    }

    /// `view` read with `query`, answered `200` with a body its schema admits: the body.
    fn rows(&self, view: &str, query: &str) -> Value {
        let (status, body) = self.read(view, query);
        assert_eq!(status, 200, "{view}?{query}: {body}");
        self.contract.validate(view, status, &body);
        body
    }

    /// The rows ESS's interpreter answers for `view` read with `params`, as JSON.
    fn interpreted(&self, view: &str, params: Value) -> Value {
        let params: BTreeMap<String, Node> =
            serde_json::from_value(params).expect("the parameters as ESS values");
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("view-list-parameter").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("the interpreter reads {view}: {error:?}"));
        json!({"rows": integral(serde_json::to_value(answer.rows).expect("rows as JSON"))})
    }
}

/// `value` with every integral float written as the integer it is: ESS's values reach JSON here
/// through `f64`, so the interpreter's `1` arrives as `1.0`, which JSON compares unequal to the
/// `1` a served answer writes.
fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if number.is_f64() && float.fract() == 0.0 && float.abs() < 9.0e15 => {
                json!(float as i64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(members) => Value::Object(
            members
                .into_iter()
                .map(|(name, member)| (name, integral(member)))
                .collect(),
        ),
        other => other,
    }
}

/// The item ids of `body`'s rows, in the order answered.
fn ids(body: &Value) -> Vec<&str> {
    body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("an answer with rows: {body}"))
        .iter()
        .map(|row| row["item_id"].as_str().expect("an item id"))
        .collect()
}

/// Fails unless `answer` is a `400` refusal naming the query parameter `wire`.
fn refused_naming(status: u16, answer: &Value, wire: &str) {
    assert_eq!(status, 400, "{answer}");
    let refused = answer["refused"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal in words: {answer}"));
    assert!(
        refused.contains(&format!("`{wire}`")) || refused.contains(&format!("query.{wire}")),
        "the refusal names the parameter {wire}: {refused}"
    );
}

#[test]
fn the_contract_publishes_each_list_parameter_as_a_repeated_query_key() {
    let board = Board::open();
    for (view, name, required) in [
        (TAGGED, "tags", true),
        (NUMBERED, "n", true),
        (MAYBE_TAGGED, "tags", false),
    ] {
        let parameter = board.contract.parameter(view, name);
        assert_eq!(parameter["in"], "query", "{view}: {parameter}");
        assert_eq!(parameter["style"], "form", "{view}: {parameter}");
        assert_eq!(parameter["explode"], true, "{view}: {parameter}");
        assert_eq!(parameter["required"], required, "{view}: {parameter}");
    }
}

#[test]
fn a_repeated_query_key_is_read_as_a_list() {
    let board = Board::open();
    let body = board.rows(TAGGED, "tags=a&tags=b");
    assert_eq!(ids(&body), ["i1", "i2", "i4"], "{body}");
    assert_eq!(body, board.interpreted(TAGGED, json!({"tags": ["a", "b"]})));

    let body = board.rows(NUMBERED, "n=1&n=3&n=5");
    assert_eq!(ids(&body), ["i1", "i3", "i5"], "{body}");
    assert_eq!(body, board.interpreted(NUMBERED, json!({"n": [1, 3, 5]})));
}

#[test]
fn a_single_key_is_a_list_of_one() {
    let board = Board::open();
    let body = board.rows(TAGGED, "tags=a");
    assert_eq!(ids(&body), ["i1", "i4"], "{body}");
    assert_eq!(body, board.interpreted(TAGGED, json!({"tags": ["a"]})));

    let body = board.rows(NUMBERED, "n=2");
    assert_eq!(ids(&body), ["i2"], "{body}");
    assert_eq!(body, board.interpreted(NUMBERED, json!({"n": [2]})));
}

#[test]
fn the_comma_form_is_one_element() {
    let board = Board::open();
    let body = board.rows(TAGGED, "tags=a,b");
    assert_eq!(
        ids(&body),
        ["i5"],
        "`tags=a,b` is the one tag `a,b`, not a list of two: {body}"
    );
    assert_eq!(body, board.interpreted(TAGGED, json!({"tags": ["a,b"]})));
}

#[test]
fn a_missing_required_list_parameter_is_refused_400() {
    let board = Board::open();
    let (status, answer) = board.read(TAGGED, "");
    refused_naming(status, &answer, "tags");
    let (status, answer) = board.read(NUMBERED, "other=1");
    refused_naming(status, &answer, "n");
}

#[test]
fn an_element_of_the_wrong_type_is_refused_400() {
    let board = Board::open();
    let (status, answer) = board.read(NUMBERED, "n=1&n=x");
    refused_naming(status, &answer, "n");
    let (status, answer) = board.read(NUMBERED, "n=1.5");
    refused_naming(status, &answer, "n");
}

#[test]
fn an_empty_element_is_refused_400() {
    let board = Board::open();
    let (status, answer) = board.read(TAGGED, "tags=");
    refused_naming(status, &answer, "tags");
    let (status, answer) = board.read(TAGGED, "tags=a&tags=");
    refused_naming(status, &answer, "tags");
    let (status, answer) = board.read(NUMBERED, "n=");
    refused_naming(status, &answer, "n");
}

#[test]
fn a_missing_optional_list_parameter_answers_unfiltered() {
    let board = Board::open();
    let body = board.rows(MAYBE_TAGGED, "");
    assert_eq!(ids(&body), ["i1", "i2", "i3", "i4", "i5"], "{body}");
    assert_eq!(body, board.interpreted(MAYBE_TAGGED, json!({})));

    let body = board.rows(MAYBE_TAGGED, "tags=c&tags=b");
    assert_eq!(ids(&body), ["i2", "i3"], "{body}");
    assert_eq!(
        body,
        board.interpreted(MAYBE_TAGGED, json!({"tags": ["c", "b"]}))
    );
}
