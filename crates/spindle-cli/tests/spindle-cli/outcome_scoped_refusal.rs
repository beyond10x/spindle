//! A command with one outcome the runtime does not run stays served: its route answers every other
//! outcome as declared, and only a request whose selected outcome is the unsupported one answers
//! `501`, naming that outcome and what it lacks, with nothing written (`story:outcome-scoped-refusal`).
//!
//! `tests/fixtures/outcome-scoped-refusal` declares one component reached over the network.
//! `PlaceBox` places a box (`placed`), or, where the request sets `weigh`, places it at a generated
//! `Decimal` weight (`weighed`), which the runtime does not mint. `LabelBox` relabels a box
//! (`labelled`), or, where the request sets `announce`, relabels it (`announced`).
//! `tests/fixtures/outcome-scoped-refusal-http.yaml` binds `LabelBox` and answers `announced` with
//! a redirect to the label, which this ingress does not write.
//!
//! `tests/fixtures/outcome-scoped-refusal-effects` declares one component reached in process, so
//! `run` reads stdin. `CloseLot` closes a lot (`closed`), or, where the request sets `sweep`,
//! closes it and sweeps every held tag the request lists (`swept`), a secondary effect over an
//! input list (`affects:` with `each:`) the runtime runs (`story:affects-over-input-list`); where
//! the sweeper jams, an external cause, it refuses the close and jams the lot (`jammed`), a
//! compensating refusal (`compensates: true`) the runtime does not run. `SweepTag` sweeps one live
//! tag. ESS refuses `sets:` without a subject (`ESS-COMMAND-003`), so no fixture declares one.
//!
//! `tests/fixtures/affects-each-network` declares one component reached over the network, whose
//! `Publish` records one row per contract it lists.

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
/// The outcome of `LabelBox` the binding answers with a redirect this ingress does not write.
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
        Self::load_spec(FIXTURE, COMPONENT, binding)
    }

    /// The document of `component` of the specification `spec`, with the binding where given.
    fn load_spec(spec: &str, component: &str, binding: Option<&str>) -> Self {
        let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|declared| declared.name.as_str() == component)
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
    serve_spec(FIXTURE, arguments)
}

/// [`serve`], serving the specification `spec`.
fn serve_spec(spec: &str, arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = state.path().join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
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
const SWEEP_TAG: &str = "bay.lot.SweepTag";
/// The outcome of `CloseLot` whose secondary effect over an input list (`affects:` with `each:`)
/// the runtime runs.
const SWEPT: &str = "bay.lot.CloseLot/swept";
/// The outcome of `CloseLot` that is a compensating refusal (`compensates: true`), which the
/// runtime does not run.
const JAMMED: &str = "bay.lot.CloseLot/jammed";

const NETWORK: &str = "crates/spindle-cli/tests/fixtures/affects-each-network";
const LEDGER: &str = "ledger";
const OPEN_RELEASE: &str = "ledger.release.OpenRelease";
const ADD_RECORD: &str = "ledger.release.AddRecord";
const PUBLISH: &str = "ledger.release.Publish";
const RECORD: &str = "ledger.release.Record";

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
    let results: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON line per request"))
        .collect();
    // A request the run refuses whole is a rejected line: the run exits 1 once every line is
    // answered (`story:run-fails-on-rejected-line`), and 0 only where none was.
    let rejected = results.iter().any(|result| result.get("failure").is_some());
    assert_eq!(output.status.code(), Some(i32::from(rejected)), "{stderr}");
    (results, stderr)
}

/// An outcome the runtime does not run, here a compensating refusal (`compensates: true`), refuses
/// only the request that selects it: selection reads no `compensates`, so `CloseLot` stays
/// served, a request selecting `jammed` is refused naming the outcome and what it lacks with
/// nothing written, and the same lot then closes through `closed`. `run` reading stdin names the
/// outcome at startup.
#[test]
fn an_outcome_with_secondary_effects_refuses_only_the_request_that_selects_it() {
    let (results, stderr) = run_reading(
        EFFECTS,
        &[
            json!({"command": "bay.lot.OpenLot", "input": {"lot_id": "l1"}}),
            json!({"command": "bay.lot.TagLot", "input": {"tag_id": "t1", "lot_id": "l1"}}),
            json!({
                "command": CLOSE_LOT,
                "input": {"lot_id": "l1", "sweep": false, "tags": []},
                "external": {"jammed": true},
            }),
            json!({
                "command": CLOSE_LOT,
                "input": {"lot_id": "l1", "sweep": false, "tags": []},
                "external": {"jammed": false},
            }),
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
        message.contains(JAMMED) && message.contains("`compensates: true`"),
        "the refusal names the outcome and what it lacks: {message}"
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
        listed[0].contains(&format!("`{JAMMED}`")) && listed[0].contains("`compensates: true`"),
        "run names the outcome and what it lacks: {listed:#?}"
    );
    assert!(
        !not_served(&lines)
            .iter()
            .any(|line| line.contains(CLOSE_LOT)),
        "run names the outcome, not the command, as unsupported: {stderr}"
    );
}

/// `admit` lists no refusal for `swept`, whose secondary effect walks an input list
/// (`story:affects-over-input-list`): the one outcome it lists is the compensating refusal.
#[test]
fn admit_lists_no_refusal_for_an_affects_entry_over_an_input_list() {
    let admitted = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["admit", EFFECTS, "--environment", LAPTOP])
        .output()
        .expect("b10x-spindle runs");
    let stdout = String::from_utf8_lossy(&admitted.stdout);
    let stderr = String::from_utf8_lossy(&admitted.stderr);
    assert_eq!(admitted.status.code(), Some(0), "{stdout}{stderr}");
    let lines = stderr.lines().map(str::to_owned).collect::<Vec<_>>();
    assert!(
        !lines.iter().any(|line| line.contains(SWEPT)),
        "admit names no refusal of `swept`: {stderr}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("`each:`")),
        "admit refuses no `affects:` entry over an input list: {stderr}"
    );
    let listed = unsupported_outcomes(&lines);
    assert_eq!(
        listed.len(),
        1,
        "admit lists the compensating refusal alone: {stderr}"
    );
    assert!(listed[0].contains(&format!("`{JAMMED}`")), "{listed:#?}");
}

/// A stdin line selecting `swept` answers `swept`: the lot is closed and every held tag the input
/// lists is swept, a tag listed but not held is not created, and a held tag not listed stays
/// `Live`. `SweepTag` then tells them apart: a swept tag answers `wrong-state`, a live one
/// `swept`, and `TagLot` creates the tag no one held.
#[test]
fn stdin_runs_an_affects_entry_over_an_input_list() {
    let tag =
        |tag: &str| json!({"command": "bay.lot.TagLot", "input": {"tag_id": tag, "lot_id": "l1"}});
    let sweep = |tag: &str| json!({"command": SWEEP_TAG, "input": {"tag_id": tag}});
    let (results, stderr) = run_reading(
        EFFECTS,
        &[
            json!({"command": "bay.lot.OpenLot", "input": {"lot_id": "l1"}}),
            tag("t1"),
            tag("t2"),
            tag("t3"),
            json!({
                "command": CLOSE_LOT,
                "input": {
                    "lot_id": "l1",
                    "sweep": true,
                    "tags": [{"tag_id": "t1"}, {"tag_id": "t9"}, {"tag_id": "t3"}],
                },
                "external": {"jammed": false},
            }),
            sweep("t1"),
            sweep("t3"),
            sweep("t2"),
            tag("t9"),
        ],
    );
    let outcomes = results
        .iter()
        .map(|result| result["outcome"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            "opened",
            "tagged",
            "tagged",
            "tagged",
            "swept",
            "wrong-state",
            "wrong-state",
            "swept",
            "tagged",
        ],
        "the close sweeps the held tags listed, t1 and t3, leaves t2 live and creates no t9: \
         {results:#?}\n{stderr}"
    );
    assert_eq!(
        results[4]["events"][0]["event"], "bay.lot.LotClosed",
        "{results:#?}"
    );
}

/// One HTTP request selecting an outcome whose `affects:` entry walks an input list answers it,
/// and `GET /api/history` answers its event for every row an element wrote: a record created and
/// a record updated alike (`Committed::instances`).
#[test]
fn http_serves_an_affects_entry_over_an_input_list() {
    let contract = Contract::load_spec(NETWORK, LEDGER, None);
    let served = serve_spec(NETWORK, &[]);
    let call = |id: &str, release: &str, body: &Value| {
        let (method, path) = contract.route(id);
        served.exchange(&method, &path.replace("{release_id}", release), Some(body))
    };

    let (status, answer) = call(OPEN_RELEASE, "r1", &json!({"release_id": "r1"}));
    assert!((200..300).contains(&status), "{status} {answer}");
    let (status, answer) = call(
        ADD_RECORD,
        "r1",
        &json!({"record_id": "k1", "release_id": "r1", "version": "v1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");

    let (status, answer) = call(
        PUBLISH,
        "r1",
        &json!({"release_id": "r1", "contracts": [
            {"record_id": "n1", "version": "a"},
            {"record_id": "k1", "version": "v2"},
            {"record_id": "n2", "version": "b"},
        ]}),
    );
    assert!(
        (200..300).contains(&status),
        "the outcome is served, not refused: {status} {answer}"
    );
    assert_eq!(answer["outcome"], "published", "{answer}");
    contract.validate(PUBLISH, status, &answer);

    for record in ["n1", "k1", "n2"] {
        let (status, history) = served.exchange(
            "GET",
            &format!("/api/history?entity={RECORD}&identity={record}"),
            None,
        );
        assert_eq!(status, 200, "{history}");
        let events = history["events"]
            .as_array()
            .unwrap_or_else(|| panic!("the console answers a history: {history}"))
            .iter()
            .map(|event| event["event"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            events.last(),
            Some(&"ledger.release.Published"),
            "the history of {record} answers the outcome's event: {history}"
        );
    }
}

/// The not-served report the Node export answers (`notServed()`,
/// `story:node-export-serves-a-specification`) is recorded from `run`: the lines it prints on
/// stderr before it listens, serving the fixture with its binding, each without `run`'s prefix.
#[test]
fn run_prints_the_recorded_not_served_lines() {
    let served = serve(&["--http", BINDING]);
    let printed: Vec<String> = served
        .stderr
        .lines()
        .iter()
        .filter_map(|line| line.strip_prefix("b10x-spindle: "))
        .filter(|line| line.starts_with("not served:") || line.starts_with("unsupported outcome"))
        .map(str::to_owned)
        .collect();
    let path = root().join("crates/spindle-node/tests/fixtures/not-served.json");
    let recorded: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}")),
    )
    .expect("the record is a list of lines");
    assert_eq!(
        printed,
        recorded,
        "run prints what crates/spindle-node/tests/fixtures/not-served.json records; it printed:\n{}",
        serde_json::to_string_pretty(&printed).unwrap_or_default()
    );
}
