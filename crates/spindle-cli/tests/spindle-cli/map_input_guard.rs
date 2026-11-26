//! The served route of a command whose input guard reads a present `Map` input answers as ESS
//! 0.99.1's interpreter decides it (`story:map-input-guard-facts`).
//!
//! The fixture is the kernel's `tests/fixtures/map-input-guard`: one component reached over the
//! network, `OpenBoard` refused `pre-scored` where `scores.count > 0` holds, `OpenLowBoard` refused
//! `high-score` where `exists: {in: scores, as: s, that: s > 5}` holds, and `OpenMixedBoard`
//! refused `all-high` where the same `forall` holds. Each request is sent to `b10x-spindle run`
//! over its derived route and run through `ess_conformance::interpret`. An outcome the interpreter
//! selects is answered at the status the contract ESS projects for the component declares, with the
//! envelope that contract's schema admits; only a request the interpreter itself refuses as
//! undecidable, an absent map, is answered `501`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Map, Value, json};

const FIXTURE: &str = "crates/spindle-kernel/tests/fixtures/map-input-guard";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "scorer";

const COUNTED: &str = "tally.board.OpenBoard";
const LOW: &str = "tally.board.OpenLowBoard";
const MIXED: &str = "tally.board.OpenMixedBoard";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&root().join(FIXTURE))
        .expect("the fixture compiles")
        .ir
}

fn object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().expect("an input object")
}

/// What ESS's interpreter answers for `input` of `command`, in a scenario of its own: the
/// outcome, or its refusal.
fn interpreted(ir: &spindle_spec::EssIr, command: &str, input: &Value) -> Result<String, String> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "tally.board/authored/map-input-guard"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("map-input-guard").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    target
        .execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: object(input)
                .into_iter()
                .map(|(field, value)| (field, serde_json::from_value(value).expect("a node")))
                .collect(),
            correlation: context.correlation.clone(),
        })
        .map(|executed| {
            executed
                .outcome
                .map(|outcome| outcome.outcome.as_str().to_owned())
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())
}

/// The `OpenAPI` document ESS projects for the fixture's component.
struct Contract(Value);

impl Contract {
    fn load(ir: &spindle_spec::EssIr) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::bound_json(ir, component, None);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method and path template of the operation `id`.
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

    /// The status the contract declares for `outcome` of `id`: the response it describes as
    /// "Outcome `<outcome>`".
    fn status(&self, id: &str, outcome: &str) -> u16 {
        let (method, path) = self.route(id);
        let responses = self.0["paths"][&path][method.to_lowercase()]["responses"]
            .as_object()
            .expect("the operation declares its responses");
        let named = format!("Outcome `{outcome}`:");
        responses
            .iter()
            .find(|(_, response)| {
                response["description"]
                    .as_str()
                    .is_some_and(|description| description.starts_with(&named))
            })
            .and_then(|(status, _)| status.parse().ok())
            .unwrap_or_else(|| panic!("the contract declares no status for {id}/{outcome}"))
    }

    /// Fails unless the contract declares `status` for `id` and `body` validates against the JSON
    /// schema it declares there.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.route(id);
        let response =
            &self.0["paths"][&path][method.to_lowercase()]["responses"][status.to_string()];
        let schema = &response["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}: {response}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
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

/// `b10x-spindle run <fixture> --environment laptop`, its state and `TMPDIR` in a scratch
/// directory, once it prints where it listens.
fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = state.path().join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", FIXTURE, "--environment", LAPTOP, "--state"])
        .arg(&file)
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
    Served {
        child,
        address,
        _state: state,
    }
}

/// One answer as written: its status and its body read as JSON.
#[derive(Debug)]
struct Reply {
    status: u16,
    body: Value,
}

impl Served {
    /// One HTTP/1.1 exchange on its own connection.
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
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("a status line: {head:?}"));
        let body = serde_json::from_str(body)
            .unwrap_or_else(|error| panic!("{error}: the body is not JSON: {answer:?}"));
        Reply { status, body }
    }

    /// The command `id` sent at its route with `input`.
    fn send(&self, contract: &Contract, id: &str, input: &Value) -> Reply {
        let (method, path) = contract.route(id);
        self.exchange(&method, &path, Some(input))
    }
}

/// The reply the served route gives `input` of `command`, after checking it against what the
/// interpreter answers for the same input: the selected outcome at the status the contract
/// declares for it, or `501` naming the guard the interpreter left `Unknown` as undecidable.
fn answered(
    ir: &spindle_spec::EssIr,
    contract: &Contract,
    served: &Served,
    command: &str,
    input: &Value,
) -> Reply {
    let reply = served.send(contract, command, input);
    match interpreted(ir, command, input) {
        Ok(outcome) => {
            assert_eq!(
                reply.status,
                contract.status(command, &outcome),
                "{command} {input}: the interpreter selects {outcome}: {reply:?}"
            );
            assert_eq!(
                reply.body["outcome"], outcome,
                "{command} {input}: {reply:?}"
            );
        }
        Err(refused) => {
            let guard = refused
                .split_once("the guard of `")
                .and_then(|(_, rest)| rest.split_once("` is Unknown"))
                .map(|(guard, _)| guard.to_owned())
                .unwrap_or_else(|| panic!("the interpreter refuses an open guard: {refused}"));
            assert_eq!(reply.status, 501, "{command} {input}: {refused}: {reply:?}");
            assert_eq!(reply.body["committed"], false, "{reply:?}");
            let named = format!("the guard of `{guard}` is undecidable");
            assert!(
                reply.body["refused"]
                    .as_str()
                    .is_some_and(|text| text.contains(&named)),
                "{command} {input}: the answer names {named}: {reply:?}"
            );
        }
    }
    contract.validate(command, reply.status, &reply.body);
    reply
}

/// A board identity of its own for the `n`th request.
fn board(n: u8) -> String {
    format!("5e7a0000-0000-4000-8000-0000000000{n:02x}")
}

#[test]
fn a_map_guard_is_served_with_the_interpreters_status() {
    let ir = ir();
    let contract = Contract::load(&ir);
    let served = serve();

    let cases = [
        (COUNTED, json!({"scores": {}}), 202, "opened"),
        (COUNTED, json!({"scores": {"a": 7}}), 422, "pre-scored"),
        (LOW, json!({"scores": {"a": 1}}), 202, "opened"),
        (LOW, json!({"scores": {"b": 9, "a": 1}}), 422, "high-score"),
        (MIXED, json!({"scores": {"a": 1}}), 202, "opened"),
        (MIXED, json!({"scores": {"b": 9, "a": 1}}), 202, "opened"),
        (MIXED, json!({"scores": {"a": 7, "b": 9}}), 422, "all-high"),
    ];
    for (n, (command, scores, status, outcome)) in (1..).zip(cases) {
        let mut input = object(&scores);
        input.insert("id".to_owned(), json!(board(n)));
        input.insert("label".to_owned(), json!(format!("board {n}")));
        let input = Value::Object(input);
        let reply = answered(&ir, &contract, &served, command, &input);
        assert_eq!(
            (reply.status, &reply.body["outcome"]),
            (status, &json!(outcome)),
            "{command} {input}: a guard over a present map is decided, not 501: {reply:?}"
        );
    }

    let unscored = answered(
        &ir,
        &contract,
        &served,
        COUNTED,
        &json!({"id": board(0xff), "label": "unscored"}),
    );
    assert_eq!(
        (unscored.status, &unscored.body["committed"]),
        (501, &json!(false)),
        "without the map the count guard stays open, as the interpreter leaves it: {unscored:?}"
    );
}
