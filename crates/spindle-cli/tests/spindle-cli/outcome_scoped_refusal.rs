//! A command with one outcome the runtime does not run stays served: its route answers every other
//! outcome as declared, and only a request whose selected outcome is the unsupported one answers
//! `501`, naming that outcome and what it lacks, with nothing written (`story:outcome-scoped-refusal`).
//!
//! `tests/fixtures/outcome-scoped-refusal` declares one component reached over the network.
//! `PlaceBox` places a box (`placed`), or, where the request sets `weigh`, places it at a generated
//! `Decimal` weight (`weighed`), which the runtime does not mint. `LabelBox` relabels a box
//! (`labelled`), or, where the request sets `announce`, relabels it under a header
//! (`announced`). `tests/fixtures/outcome-scoped-refusal-http.yaml` binds `LabelBox` and declares
//! that header for `announced`, which this ingress does not write.
//!
//! `tests/fixtures/outcome-scoped-refusal-effects` declares one component reached in process, so
//! `run` reads stdin. `CloseLot` closes a lot (`closed`), or, where the request sets `sweep`,
//! closes it and sweeps the tags the request lists (`swept`), a secondary effect over an input list
//! (`affects:` with `each:`) the runtime does not run. ESS refuses `sets:` without a subject
//! (`ESS-COMMAND-003`), so no fixture declares one.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const FIXTURE: &str = "crates/spindle-cli/tests/fixtures/outcome-scoped-refusal";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/outcome-scoped-refusal-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "shelf";

const PLACE_BOX: &str = "shelf.box.PlaceBox";
const LABEL_BOX: &str = "shelf.box.LabelBox";
const ALL_BOXES: &str = "shelf.box.AllBoxes";
/// The outcome of `PlaceBox` whose generated `Decimal` weight the runtime does not mint.
const WEIGHED: &str = "shelf.box.PlaceBox/weighed";
/// The outcome of `LabelBox` the binding answers with a header this ingress does not write.
const ANNOUNCED: &str = "shelf.box.LabelBox/announced";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);
/// How often a wait looks again.
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for the fixture's component, with the binding where given.
struct Contract(Value);

impl Contract {
    fn load(binding: Option<&str>) -> Self {
        let loaded = spindle_spec::load(&root().join(FIXTURE)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let binding = binding.map(|binding| {
            let text = std::fs::read_to_string(root().join(binding)).expect("the binding is read");
            let parsed = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
            let http = ess_http::compile(&parsed, &loaded.ir, &ess_gen::http::derived(&loaded.ir))
                .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
            ess_gen::http::Binding::new(http)
                .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"))
        });
        let document = ess_gen::openapi::bound_json(&loaded.ir, component, binding.as_ref());
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

    /// Fails unless `body` validates against the schema the contract declares for `id` under
    /// `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.route(id);
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
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
}

/// Every line `run` wrote on stderr so far, read on a thread of its own.
#[derive(Clone, Default)]
struct Stderr(Arc<Mutex<Vec<String>>>);

impl Stderr {
    fn read(child: &mut Child) -> Self {
        let lines = Self::default();
        let stderr = child.stderr.take().expect("stderr is piped");
        let filled = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                filled.0.lock().expect("the stderr lines").push(line);
            }
        });
        lines
    }

    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("the stderr lines").clone()
    }
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: Stderr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `b10x-spindle run <fixture> --environment laptop <arguments>`, its state and `TMPDIR` in a
/// scratch directory, once it prints where it listens. Everything it prints on stderr before that
/// line is read by then.
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
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = Stderr::read(&mut child);
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
        panic!(
            "run's first line {line:?} is not JSON ({error}); stderr: {:#?}",
            stderr.lines()
        )
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    let served = Served {
        child,
        address,
        stderr,
        _state: state,
    };
    served.settled();
    served
}

impl Served {
    /// Waits until stderr has stopped growing for a moment: the startup lines are printed before
    /// the listening line, but read on their own thread.
    fn settled(&self) {
        let deadline = Instant::now() + ANSWER;
        let mut seen = self.stderr.lines().len();
        let mut quiet = Instant::now();
        while quiet.elapsed() < Duration::from_millis(200) && Instant::now() < deadline {
            std::thread::sleep(POLL);
            let now = self.stderr.lines().len();
            if now != seen {
                seen = now;
                quiet = Instant::now();
            }
        }
    }

    /// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
    fn exchange(&self, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
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
            .split(' ')
            .nth(1)
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("a status line: {head:?}"));
        let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
        (status, body)
    }

    /// The operation `id` asked at its route, `box_id` filling the path's one variable if it has
    /// one.
    fn call(
        &self,
        contract: &Contract,
        id: &str,
        box_id: &str,
        body: Option<&Value>,
    ) -> (u16, Value) {
        let (method, path) = contract.route(id);
        self.exchange(&method, &path.replace("{box_id}", box_id), body)
    }

    fn events(&self) -> Vec<Value> {
        let (status, body) = self.exchange("GET", "/api/events", None);
        assert_eq!(status, 200, "{body}");
        body["events"]
            .as_array()
            .unwrap_or_else(|| panic!("the console answers events: {body}"))
            .clone()
    }

    fn rows(&self, contract: &Contract) -> Value {
        let (status, rows) = self.call(contract, ALL_BOXES, "", None);
        assert_eq!(status, 200, "{rows}");
        contract.validate(ALL_BOXES, status, &rows);
        rows
    }
}

/// The lines of `stderr` that name an operation as not served.
fn not_served(stderr: &[String]) -> Vec<String> {
    stderr
        .iter()
        .filter(|line| line.contains("not served:"))
        .cloned()
        .collect()
}

/// The lines of `stderr` that name an outcome as unsupported.
fn unsupported_outcomes(stderr: &[String]) -> Vec<String> {
    stderr
        .iter()
        .filter(|line| line.contains("unsupported outcome"))
        .cloned()
        .collect()
}

/// Fails unless `answer` is ESS's closed `501` envelope, nothing committed, naming `outcome` and
/// each of `missing`.
fn refused_naming(answer: &Value, outcome: &str, missing: &[&str]) {
    assert_eq!(
        answer.as_object().map(|members| members.len()),
        Some(2),
        "ESS's 501 envelope is closed over refused and committed: {answer}"
    );
    assert_eq!(answer["committed"], false, "{answer}");
    let refused = answer["refused"].as_str().expect("a refusal in words");
    assert!(
        refused.contains(outcome),
        "the 501 names the outcome {outcome}: {refused}"
    );
    for what in missing {
        assert!(
            refused.contains(what),
            "the 501 names what is missing, {what}: {refused}"
        );
    }
}

#[test]
fn a_route_with_one_unsupported_outcome_serves_its_other_outcomes() {
    let contract = Contract::load(None);
    let served = serve(&[]);

    let (status, answer) = served.call(
        &contract,
        PLACE_BOX,
        "",
        Some(&json!({"box_id": "b1", "label": "kept", "weigh": false})),
    );
    assert!(
        (200..300).contains(&status),
        "the default outcome answers as declared: {status} {answer}"
    );
    assert_eq!(answer["outcome"], "placed", "{answer}");
    contract.validate(PLACE_BOX, status, &answer);
    let published = served.events();
    assert_eq!(published.len(), 1, "{published:?}");
    assert_eq!(published[0]["event"], "shelf.box.BoxPlaced");
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"box_id": "b1", "label": "kept"}]})
    );
    assert!(
        !not_served(&served.stderr.lines())
            .iter()
            .any(|line| line.contains(PLACE_BOX)),
        "the command stays served: {:#?}",
        served.stderr.lines()
    );
}

#[test]
fn the_unsupported_outcome_answers_501_when_chosen() {
    let contract = Contract::load(None);
    let served = serve(&[]);

    let (status, answer) = served.call(
        &contract,
        PLACE_BOX,
        "",
        Some(&json!({"box_id": "b2", "label": "heavy", "weigh": true})),
    );
    assert_eq!(status, 501, "{answer}");
    contract.validate(PLACE_BOX, status, &answer);
    refused_naming(&answer, WEIGHED, &["`weight`", "Decimal"]);
    assert_eq!(
        served.events(),
        Vec::<Value>::new(),
        "nothing was published"
    );
    assert_eq!(
        served.rows(&contract),
        json!({"rows": []}),
        "nothing was written"
    );

    let (status, answer) = served.call(
        &contract,
        PLACE_BOX,
        "",
        Some(&json!({"box_id": "b2", "label": "light", "weigh": false})),
    );
    assert!(
        (200..300).contains(&status),
        "the box the refused request named was never created, so it is placed now: {status} \
         {answer}"
    );
    assert_eq!(answer["outcome"], "placed", "{answer}");
}

#[test]
fn admission_lists_the_unsupported_outcome() {
    let admitted = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["admit", FIXTURE, "--environment", LAPTOP])
        .output()
        .expect("b10x-spindle runs");
    let stdout = String::from_utf8_lossy(&admitted.stdout);
    let stderr = String::from_utf8_lossy(&admitted.stderr);
    assert_eq!(admitted.status.code(), Some(0), "{stdout}{stderr}");
    let lines = stderr.lines().map(str::to_owned).collect::<Vec<_>>();
    let listed = unsupported_outcomes(&lines);
    assert_eq!(listed.len(), 1, "admit lists the one outcome: {stderr}");
    assert!(
        listed[0].contains(&format!("`{WEIGHED}`"))
            && listed[0].contains("`weight`")
            && listed[0].contains("Decimal"),
        "admit names the outcome and what it lacks: {listed:#?}"
    );
    assert!(
        lines
            .iter()
            .filter(|line| line.contains(PLACE_BOX))
            .all(|line| line.contains(WEIGHED)),
        "admit names the outcome, not the command: {stderr}"
    );

    let json = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["admit", FIXTURE, "--environment", LAPTOP, "--json"])
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(json.status.code(), Some(0), "{json:?}");
    let admission: Value =
        serde_json::from_slice(&json.stdout).expect("admit --json prints JSON alone on stdout");
    assert_eq!(admission["outcome"], "admitted", "{admission}");
    assert_eq!(
        unsupported_outcomes(
            &String::from_utf8_lossy(&json.stderr)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        )
        .len(),
        1,
        "{json:?}"
    );

    let served = serve(&[]);
    let lines = served.stderr.lines();
    let listed = unsupported_outcomes(&lines);
    assert_eq!(listed.len(), 1, "run lists the one outcome: {lines:#?}");
    assert!(
        listed[0].contains(&format!("`{WEIGHED}`"))
            && listed[0].contains("`weight`")
            && listed[0].contains("Decimal"),
        "run names the outcome and what it lacks: {listed:#?}"
    );
    assert!(
        !not_served(&lines)
            .iter()
            .any(|line| line.contains(PLACE_BOX)),
        "run names the outcome, not the command, as unsupported: {lines:#?}"
    );
}

/// The outcome a binding answers in a way this ingress does not write refuses only when a request
/// selects it, before anything is written; the command's other outcomes answer as bound.
#[test]
fn an_outcome_the_ingress_does_not_answer_refuses_only_when_chosen() {
    let contract = Contract::load(Some(BINDING));
    let served = serve(&["--http", BINDING]);

    let lines = served.stderr.lines();
    let listed = unsupported_outcomes(&lines);
    assert!(
        listed
            .iter()
            .any(|line| line.contains(&format!("`{ANNOUNCED}`"))),
        "run lists the outcome the ingress does not answer: {lines:#?}"
    );
    assert!(
        !not_served(&lines)
            .iter()
            .any(|line| line.contains(LABEL_BOX)),
        "the command stays served: {lines:#?}"
    );

    let (status, answer) = served.call(
        &contract,
        PLACE_BOX,
        "",
        Some(&json!({"box_id": "b1", "label": "kept", "weigh": false})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let (status, answer) = served.call(
        &contract,
        LABEL_BOX,
        "b1",
        Some(&json!({"label": "plain", "announce": false})),
    );
    assert_eq!(status, 200, "the bound status: {answer}");
    assert_eq!(answer["outcome"], "labelled", "{answer}");
    contract.validate(LABEL_BOX, status, &answer);
    let published = served.events();

    let (status, answer) = served.call(
        &contract,
        LABEL_BOX,
        "b1",
        Some(&json!({"label": "loud", "announce": true})),
    );
    assert_eq!(status, 501, "{answer}");
    contract.validate(LABEL_BOX, status, &answer);
    refused_naming(&answer, ANNOUNCED, &["this ingress does not serve"]);
    assert_eq!(served.events(), published, "nothing was published");
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"box_id": "b1", "label": "plain"}]}),
        "nothing was written"
    );
}

const EFFECTS: &str = "crates/spindle-cli/tests/fixtures/outcome-scoped-refusal-effects";
const CLOSE_LOT: &str = "bay.lot.CloseLot";
/// The outcome of `CloseLot` whose secondary effect over an input list (`affects:` with `each:`)
/// the runtime does not run.
const SWEPT: &str = "bay.lot.CloseLot/swept";

/// `b10x-spindle run <spec> --environment laptop` with `requests` on stdin, one per line: the
/// result lines, parsed, and stderr.
fn run_reading(spec: &str, requests: &[Value]) -> (Vec<Value>, String) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", spec, "--environment", LAPTOP])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let lines = requests
        .iter()
        .map(|request| format!("{request}\n"))
        .collect::<String>();
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the requests are written");
    let output = child.wait_with_output().expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    let results = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON line per request"))
        .collect();
    (results, stderr)
}

/// An outcome with a secondary effect the runtime does not run (`affects:` with `each:`) refuses
/// only the request that selects it: selection reads no `affects:`, so `CloseLot` stays served, a
/// request selecting `swept` is refused naming the outcome and its effect with nothing written,
/// and the same lot then closes through `closed`. `run` reading stdin names the outcome at
/// startup.
#[test]
fn an_outcome_with_secondary_effects_refuses_only_the_request_that_selects_it() {
    let (results, stderr) = run_reading(
        EFFECTS,
        &[
            json!({"command": "bay.lot.OpenLot", "input": {"lot_id": "l1"}}),
            json!({"command": "bay.lot.TagLot", "input": {"tag_id": "t1", "lot_id": "l1"}}),
            json!({"command": CLOSE_LOT, "input": {"lot_id": "l1", "sweep": true, "tags": [{"tag_id": "t1"}]}}),
            json!({"command": CLOSE_LOT, "input": {"lot_id": "l1", "sweep": false, "tags": []}}),
        ],
    );
    assert_eq!(results.len(), 4, "{results:#?}");
    assert_eq!(results[0]["outcome"], "opened", "{results:#?}");
    assert_eq!(results[1]["outcome"], "tagged", "{results:#?}");
    assert_eq!(results[2]["failure"]["kind"], "unsupported", "{results:#?}");
    let message = results[2]["failure"]["message"]
        .as_str()
        .expect("a refusal in words");
    assert!(
        message.contains(SWEPT) && message.contains("`affects:`"),
        "the refusal names the outcome and its secondary effect: {message}"
    );
    assert_eq!(
        results[3]["outcome"], "closed",
        "the refused request wrote nothing, so the lot is still open and closes now; a request \
         selecting the supported outcome runs: {:?}",
        results[3]
    );

    let lines = stderr.lines().map(str::to_owned).collect::<Vec<_>>();
    let listed = unsupported_outcomes(&lines);
    assert_eq!(listed.len(), 1, "run lists the one outcome: {stderr}");
    assert!(
        listed[0].contains(&format!("`{SWEPT}`")) && listed[0].contains("`affects:`"),
        "run names the outcome and what it lacks: {listed:#?}"
    );
    assert!(
        !not_served(&lines)
            .iter()
            .any(|line| line.contains(CLOSE_LOT)),
        "run names the outcome, not the command, as unsupported: {stderr}"
    );
}
