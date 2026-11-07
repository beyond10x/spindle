//! Adversary cases for `story:outcome-scoped-refusal`: what a request whose selected outcome the
//! runtime does not run leaves behind, over HTTP and through a binding, and what it answers
//! beside a wrong-state answer and a grant refusal.
//!
//! `tests/fixtures/adversary-outcome-scoped-refusal` declares one component reached over the
//! network. `PackCrate` packs a crate under a minted identity (`packed`), or, where the request
//! sets `weigh`, at a generated `Decimal` weight the runtime does not mint (`weighed`).
//! `SealCrate` seals an open crate (`sealed`), or with `weigh` at such a weight (`weighed`), and
//! answers `wrong-state` for a crate that is not open. `WeighCrate` has one outcome, which the
//! runtime does not run. The actor `Packer` may send `PackCrate` alone.
//! `tests/fixtures/adversary-outcome-scoped-refusal-http.yaml` binds `SealCrate` and answers
//! `wrong-state` with a header this ingress does not write.
//! `tests/fixtures/adversary-outcome-scoped-refusal-bound` takes parcels over the network and
//! records each through a retrying binding, weighed there only where the sender opts in.
//!
//! Whether `admit`, `run` over HTTP and `run` reading stdin say the same thing about an
//! outcome the runtime does not run, and whether a compensating refusal, which selection never
//! reads, refuses only the request that selects it.
//! `tests/fixtures/adversary-outcome-scoped-refusal-stdin` is the model above with its component
//! reached in process, so `run` reads stdin.
//! `tests/fixtures/adversary-outcome-scoped-refusal-compensates` declares `JoinOrder`, whose
//! `failed` is an external compensating refusal, as ESS's own `refusal-with-effect` fixture
//! declares it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub(super) const FIXTURE: &str =
    "crates/spindle-cli/tests/fixtures/adversary-outcome-scoped-refusal";
const BINDING: &str =
    "crates/spindle-cli/tests/fixtures/adversary-outcome-scoped-refusal-http.yaml";
pub(super) const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "yard";

pub(super) const PACK: &str = "yard.crate.PackCrate";
pub(super) const SEAL: &str = "yard.crate.SealCrate";
const PACKER: &str = "yard.crate.Packer";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);

pub(super) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The method and path ESS projects for the operation `id`, with the binding where given.
fn route(binding: Option<&str>, id: &str) -> (String, String) {
    route_in(FIXTURE, COMPONENT, binding, id)
}

/// [`route`], of `component` in the specification `spec`.
fn route_in(spec: &str, component: &str, binding: Option<&str>, id: &str) -> (String, String) {
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
    let document: Value = serde_json::from_str(&ess_gen::openapi::bound_json(
        &loaded.ir,
        component,
        binding.as_ref(),
    ))
    .expect("the projection is JSON");
    for (path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("a path item") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

#[derive(Clone, Default)]
pub(super) struct Stderr(Arc<Mutex<Vec<String>>>);

impl Stderr {
    pub(super) fn lines(&self) -> Vec<String> {
        self.0.lock().expect("the stderr lines").clone()
    }
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
pub(super) struct Served {
    child: Child,
    address: SocketAddr,
    pub(super) stderr: Stderr,
    binding: Option<&'static str>,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn serve(binding: Option<&'static str>, arguments: &[&str]) -> Served {
    serve_spec(FIXTURE, binding, arguments)
}

/// [`serve`], running the specification `spec`.
fn serve_spec(spec: &str, binding: Option<&'static str>, arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments);
    if let Some(binding) = binding {
        command.args(["--http", binding]);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = Stderr::default();
    let filled = stderr.clone();
    let pipe = child.stderr.take().expect("stderr is piped");
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let Ok(line) = line else { break };
            filled.0.lock().expect("the stderr lines").push(line);
        }
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "{line:?} is not JSON ({error}); stderr: {:#?}",
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
        binding,
        _state: state,
    };
    let deadline = Instant::now() + ANSWER;
    let mut seen = served.stderr.lines().len();
    let mut quiet = Instant::now();
    while quiet.elapsed() < Duration::from_millis(200) && Instant::now() < deadline {
        std::thread::sleep(POLL);
        let now = served.stderr.lines().len();
        if now != seen {
            seen = now;
            quiet = Instant::now();
        }
    }
    served
}

impl Served {
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

    pub(super) fn call(&self, id: &str, body: &Value) -> (u16, Value) {
        let (method, path) = route(self.binding, id);
        self.exchange(&method, &path, Some(body))
    }

    fn events(&self) -> Vec<Value> {
        let (status, body) = self.exchange("GET", "/api/events", None);
        assert_eq!(status, 200, "{body}");
        body["events"].as_array().expect("events").clone()
    }

    fn reset(&self, world: &Value) {
        let (status, body) = self.exchange("POST", "/api/reset", Some(world));
        assert_eq!(status, 200, "{body}");
    }

    /// Packs one crate unweighed and answers its minted identity.
    fn pack(&self, label: &str) -> Value {
        let (status, answer) = self.call(PACK, &json!({"label": label, "weigh": false}));
        assert!((200..300).contains(&status), "{status} {answer}");
        assert_eq!(answer["outcome"], "packed", "{answer}");
        answer["published"][0]["payload"]["crate_id"].clone()
    }
}

/// A request whose selected outcome is refused mints nothing: the seeded mint a reset fixes
/// hands the next supported request the identity it would have handed it had the refused request
/// never come.
#[test]
fn a_refused_request_draws_nothing_from_the_seeded_mint() {
    let served = serve(None, &[]);

    served.reset(&json!({"mint": 7}));
    let (status, answer) = served.call(PACK, &json!({"label": "heavy", "weigh": true}));
    assert_eq!(status, 501, "{answer}");
    assert_eq!(answer["committed"], false, "{answer}");
    assert!(
        answer["refused"]
            .as_str()
            .is_some_and(|refused| refused.contains(&format!("{PACK}/weighed"))),
        "{answer}"
    );
    let after_refusal = served.pack("light");
    assert_eq!(served.events().len(), 1, "only the packed crate published");

    served.reset(&json!({"mint": 7}));
    let alone = served.pack("light");
    assert!(after_refusal.is_string(), "{after_refusal}");
    assert_eq!(
        after_refusal, alone,
        "the refused request drew an identity or a value from the mint before it was refused \
         (story:outcome-scoped-refusal)"
    );
}

/// A request that selects the unsupported branch of a move from a state the crate does not hold
/// answers what the same request selecting the supported branch answers: the wrong-state
/// refusal, as a command with no unsupported outcome would.
#[test]
fn a_sealed_crate_answers_wrong_state_whichever_branch_the_request_selects() {
    let served = serve(None, &[]);
    let crate_id = served.pack("kept");
    let (status, answer) = served.call(SEAL, &json!({"crate_id": crate_id, "weigh": false}));
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "sealed", "{answer}");
    let published = served.events();

    let plain = served.call(SEAL, &json!({"crate_id": crate_id, "weigh": false}));
    let weighed = served.call(SEAL, &json!({"crate_id": crate_id, "weigh": true}));
    assert_eq!(plain.1["outcome"], "wrong-state", "{plain:?}");
    assert_eq!(
        weighed, plain,
        "the wrong-state answer, not the 501, for a branch whose move does not apply \
         (story:outcome-scoped-refusal)"
    );
    assert_eq!(served.events(), published, "nothing was published");
}

/// An actor whose grant does not name the command is refused `403` before its selected outcome
/// is looked at, as for a command with no unsupported outcome.
#[test]
fn an_actor_not_granted_is_refused_before_the_unsupported_outcome() {
    let served = serve(None, &["--fixed-actor", PACKER]);
    let crate_id = served.pack("kept");
    let published = served.events();

    let plain = served.call(SEAL, &json!({"crate_id": crate_id, "weigh": false}));
    let weighed = served.call(SEAL, &json!({"crate_id": crate_id, "weigh": true}));
    assert_eq!(plain.0, 403, "{plain:?}");
    assert_eq!(
        weighed, plain,
        "ESS's not-granted refusal, not the 501 (story:outcome-scoped-refusal)"
    );
    assert_eq!(served.events(), published, "nothing was published");
}

/// The `wrong-state` refusal the binding answers with a header this ingress does not write is
/// refused where a request selects it, naming it, through the wrong-state path of the kernel
/// (`Run::refusal`); the command's other outcomes answer as bound.
#[test]
fn a_bound_wrong_state_answer_the_ingress_does_not_write_refuses_where_selected() {
    let served = serve(Some(BINDING), &[]);
    let listed = served
        .stderr
        .lines()
        .into_iter()
        .filter(|line| line.contains(&format!("`{SEAL}/wrong-state`")))
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 1, "{:#?}", served.stderr.lines());

    let crate_id = served.pack("kept");
    let (status, answer) = served.exchange(
        "PUT",
        "/crates/seal",
        Some(&json!({"crate_id": crate_id, "weigh": false})),
    );
    assert_eq!(status, 200, "the bound status: {answer}");
    assert_eq!(answer["outcome"], "sealed", "{answer}");
    let published = served.events();

    let (status, answer) = served.exchange(
        "PUT",
        "/crates/seal",
        Some(&json!({"crate_id": crate_id, "weigh": false})),
    );
    assert_eq!(status, 501, "{answer}");
    assert_eq!(answer["committed"], false, "{answer}");
    let refused = answer["refused"].as_str().expect("a refusal in words");
    assert!(
        refused.contains(&format!("`{SEAL}/wrong-state`"))
            && refused.contains("this ingress does not serve"),
        "the 501 names the wrong-state outcome and why: {refused}"
    );
    assert_eq!(served.events(), published, "nothing was published");
}

const BOUND: &str = "crates/spindle-cli/tests/fixtures/adversary-outcome-scoped-refusal-bound";
const TAKE: &str = "dock.desk.Take";
const RECORD: &str = "dock.scale.Record";

/// An outcome reached through a binding rather than over HTTP is refused the same way: the
/// request that took the parcel stands, the binding's command writes and publishes nothing for
/// the outcome it selected, on every attempt its retry makes, and is dropped naming that
/// outcome; the next parcel, whose binding selects the supported outcome, is recorded.
#[test]
fn an_outcome_a_binding_selects_is_refused_without_writing_half() {
    let served = serve_spec(BOUND, None, &[]);
    let (method, path) = route_in(BOUND, "desk", None, TAKE);

    let (status, answer) = served.exchange(
        &method,
        &path,
        Some(&json!({"parcel_id": "p1", "weigh": true})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "taken", "{answer}");
    let names = |events: &[Value]| {
        events
            .iter()
            .map(|event| event["event"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&served.events()),
        ["dock.desk.Taken"],
        "the binding's refused outcome published nothing"
    );
    let deadline = Instant::now() + ANSWER;
    let dropped = loop {
        let dropped = served
            .stderr
            .lines()
            .into_iter()
            .filter(|line| line.contains(&format!("did not run {RECORD}")))
            .collect::<Vec<_>>();
        if !dropped.is_empty() || Instant::now() > deadline {
            break dropped;
        }
        std::thread::sleep(POLL);
    };
    assert_eq!(dropped.len(), 1, "{:#?}", served.stderr.lines());
    assert!(
        dropped[0].contains(&format!("`{RECORD}/weighed`")),
        "the drop names the refused outcome: {}",
        dropped[0]
    );

    let (status, answer) = served.exchange(
        &method,
        &path,
        Some(&json!({"parcel_id": "p2", "weigh": false})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let published = served.events();
    assert_eq!(
        names(&published),
        ["dock.desk.Taken", "dock.desk.Taken", "dock.scale.Recorded"],
        "{published:?}"
    );
    assert_eq!(
        published[2]["payload"]["parcel_id"], "p2",
        "only the parcel whose binding selected the supported outcome is recorded: {published:?}"
    );
}

const STDIN_FIXTURE: &str =
    "crates/spindle-cli/tests/fixtures/adversary-outcome-scoped-refusal-stdin";
const WEIGH: &str = "yard.crate.WeighCrate";

fn admit_stderr() -> Vec<String> {
    let admitted = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["admit", FIXTURE, "--environment", LAPTOP])
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(admitted.status.code(), Some(0), "{admitted:?}");
    String::from_utf8_lossy(&admitted.stderr)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// `WeighCrate` has no outcome the runtime runs, so it is refused whatever the request: `run`
/// names the command as not served, and a request whose input is not even valid answers `501`,
/// not the `400` an outcome-scoped refusal leaves it. `admit` must not tell the operator the
/// opposite, that the outcome is refused only where a request selects it.
#[test]
fn admit_and_run_agree_on_a_command_none_of_whose_outcomes_runs() {
    let served = serve(None, &[]);
    let lines = served.stderr.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("not served") && line.contains(WEIGH)),
        "run names the command as not served: {lines:#?}"
    );
    let (status, answer) = served.call(WEIGH, &json!({"crate_id": "not-a-uuid"}));
    assert_eq!(
        status, 501,
        "the whole command is refused before its input is read: {answer}"
    );
    let (status, answer) = served.call(SEAL, &json!({"crate_id": "not-a-uuid", "weigh": true}));
    assert_eq!(
        status, 400,
        "an outcome-scoped refusal reads the input first: {answer}"
    );

    let admitted = admit_stderr();
    let scoped = admitted
        .iter()
        .filter(|line| {
            line.contains(&format!("`{WEIGH}/weighed`"))
                && line.contains("refused where a request selects it")
        })
        .collect::<Vec<_>>();
    assert!(
        scoped.is_empty(),
        "admit says a request selecting {WEIGH}/weighed is refused and the command's other \
         outcomes run, while run refuses every request of {WEIGH} as not served: {scoped:#?}"
    );
}

fn run_stdin(lines: &str) -> Output {
    run_stdin_spec(STDIN_FIXTURE, lines)
}

/// `b10x-spindle run <spec> --environment laptop`, `lines` on stdin.
fn run_stdin_spec(spec: &str, lines: &str) -> Output {
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
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the commands are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

/// `run` reading stdin refuses the request that selects the unsupported outcome, naming it, and
/// runs the command's other outcome. The story's acceptance has `run`'s startup name each such
/// outcome, as `admit` and `run` over HTTP do.
#[test]
fn run_reading_stdin_names_each_unsupported_outcome_at_startup() {
    let output = run_stdin(concat!(
        r#"{"command": "yard.crate.PackCrate", "input": {"label": "heavy", "weigh": true}}"#,
        "\n",
        r#"{"command": "yard.crate.PackCrate", "input": {"label": "light", "weigh": false}}"#,
        "\n",
    ));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("one JSON line per request"))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[0]["failure"]["kind"], "unsupported", "{results:?}");
    assert!(
        results[0]["failure"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&format!("{PACK}/weighed"))),
        "{results:?}"
    );
    assert_eq!(results[1]["outcome"], "packed", "{results:?}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("unsupported outcome")
                && line.contains(&format!("`{PACK}/weighed`"))),
        "run reading stdin names no unsupported outcome at startup \
         (story:outcome-scoped-refusal acceptance: `run` startup names the outcome): {stderr}"
    );
}

const COMPENSATES: &str =
    "crates/spindle-cli/tests/fixtures/adversary-outcome-scoped-refusal-compensates";

/// `JoinOrder`'s `failed` is a compensating refusal (`compensates: true`), the shape of ESS's own
/// `refusal-with-effect` fixture, which the kernel does not run. Selection never reads
/// `compensates` (neither the kernel's `select` nor ESS's precedence plan), so it is a construct
/// only the taken outcome reads: a request the upstream does not refuse selects `joined`, which
/// the kernel runs, and answers as a command without `failed` would.
#[test]
fn a_compensating_refusal_refuses_only_the_request_that_selects_it() {
    const ORDER: &str = "6f1c0a52-3d4e-4b7a-9c1d-2e5f8a9b0c7d";
    let output = run_stdin_spec(
        COMPENSATES,
        &[
            json!({"command": "shop.order.PlaceOrder", "input": {"order_id": ORDER}}),
            json!({
                "command": "shop.order.JoinOrder",
                "input": {"order_id": ORDER, "reason": "upstream down"},
                "external": {"failed": false},
            }),
        ]
        .iter()
        .map(|line| format!("{line}\n"))
        .collect::<String>(),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("one JSON line per request"))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[0]["outcome"], "placed", "{results:?}");
    assert_eq!(
        results[1]["outcome"], "joined",
        "a request that selects the supported outcome runs; only `failed` is one the kernel \
         does not run (story:outcome-scoped-refusal): {:?}",
        results[1]
    );
}
