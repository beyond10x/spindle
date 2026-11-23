//! `run --seed <file>`, or the environment document's `[seed] file`: a JSON array of entries
//! `{command, input, actor, caller, request_id}` that a served run submits after its stores are
//! open, one at a time in the file's order, beside the listener, on every start
//! (`story:seed-at-start`). Each entry counts as applied, already there (an outcome the
//! specification declares `existing_instance:`, or a request id an earlier entry carried) or
//! refused; the counts are in `GET /__spindle/metrics` and in one log line when the seed ends.
//!
//! The fixture `tests/fixtures/seed` declares one component, `gate`, reached over the network.
//! `GrantRole` creates a role for the caller's `tenant` and answers a role already held with its
//! `existing_instance:` duplicate; `RetireRole` refuses an unknown role with `RoleMissing`;
//! `VouchFor` asks an external cause, which a desk this test runs answers. The actor `Operator`
//! carries `tenant`; `Visitor` carries nothing and may only vouch.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/seed";
const IN_PROCESS: &str = "crates/spindle-cli/tests/fixtures/http-cause";
const GRANT: &str = "vault.access.GrantRole";
const RETIRE: &str = "vault.access.RetireRole";
const VOUCH: &str = "vault.access.VouchFor";
const OPERATOR: &str = "vault.access.Operator";
const VISITOR: &str = "vault.access.Visitor";
const CAPABILITY: &str = "external.vault.access.VouchFor.vouched";
const HEADER: &str = "X-Spindle-Caller";
const SECRET_HEADER: &str = "Spindle-Proxy-Secret";
const SECRET: &str = "seed-proxy-secret-4c1d";

/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one exchange, or the seed's end, may take.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// Writes `text` to `name` in `directory`; answers its path.
fn write(directory: &tempfile::TempDir, name: &str, text: &str) -> String {
    let path = directory.path().join(name);
    std::fs::write(&path, text).expect("the file is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

/// Writes a served environment document to `directory`: state in `store`, `VouchFor`'s cause bound
/// to the desk at `desk` where given, and `extra` appended.
fn environment(
    directory: &tempfile::TempDir,
    store: &str,
    desk: Option<SocketAddr>,
    extra: &str,
) -> String {
    let call = desk.map_or_else(String::new, |desk| {
        format!(
            "\"{CAPABILITY}\" = [\"desk\"]\n\n[http.desk]\nmethod = \"POST\"\n\
             url = \"http://{desk}/vouch\"\ntimeout_ms = 30000\n"
        )
    });
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"seeded\"\nos = \"linux\"\narch = \"x86_64\"\n\
         {extra}\n[provides]\n\"store.durable\" = [\"{store}\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\n\
         channel = [\"in-process\"]\nprojection = [\"in-process\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"external.*\" = [\"operator-prompt\"]\n{call}"
    );
    write(directory, "environment.toml", &text)
}

fn grant(role: &str, tenant: &str, request: &str) -> Value {
    json!({
        "command": GRANT,
        "input": {"role_id": role, "title": format!("{role} title")},
        "actor": OPERATOR,
        "caller": {"tenant": tenant},
        "request_id": request,
    })
}

/// A running `b10x-spindle run`; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    /// Each line it printed on stderr, as printed.
    stderr: mpsc::Receiver<String>,
    /// Every line read from `stderr` so far.
    seen: Vec<String>,
}

impl Served {
    /// Waits for a stderr line holding `text`; answers it.
    fn line(&mut self, text: &str) -> String {
        if let Some(line) = self.seen.iter().find(|line| line.contains(text)) {
            return line.clone();
        }
        let deadline = Instant::now() + ANSWER;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(line) = self.stderr.recv_timeout(left) else {
                panic!(
                    "no stderr line holds {text:?} within {ANSWER:?}: {:#?}",
                    self.seen
                );
            };
            self.seen.push(line.clone());
            if line.contains(text) {
                return line;
            }
        }
    }

    /// The seed's end line.
    fn ended(&mut self) -> String {
        self.line(" ended: ")
    }

    /// Stops the run and answers every line it printed on stderr.
    fn stop(mut self) -> Vec<String> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        while let Ok(line) = self.stderr.recv_timeout(ANSWER) {
            self.seen.push(line);
        }
        std::mem::take(&mut self.seen)
    }

    fn events(&self, headers: &[(&str, &str)]) -> Vec<Value> {
        let (status, body) = exchange(self.address, "GET", "/api/events", headers);
        assert_eq!(status, 200, "{body}");
        let events: Value = serde_json::from_str(&body).expect("the events are JSON");
        events["events"].as_array().cloned().unwrap_or_default()
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on `spec` with `arguments`: the running process, or the exit code, stdout and stderr of
/// one that stopped without listening.
fn start(spec: &str, arguments: &[&str]) -> Result<Served, (Option<i32>, String, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .arg("run")
        .arg(spec)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (printed, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { return };
            if printed.send(line).is_err() {
                return;
            }
        }
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let mut rest = String::new();
        if line.is_empty() {
            let _ = reader.read_to_string(&mut rest);
        }
        let _ = sender.send((line, rest));
    });
    let Ok((line, rest)) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let mut stderr = Vec::new();
        while let Ok(line) = lines.recv_timeout(ANSWER) {
            stderr.push(line);
        }
        return Err((status.code(), rest, stderr.join("\n")));
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Ok(Served {
        child,
        address,
        stderr: lines,
        seen: Vec::new(),
    })
}

fn serve(arguments: &[&str]) -> Served {
    start(SPEC, arguments).unwrap_or_else(|stopped| panic!("run did not listen: {stopped:?}"))
}

/// One HTTP/1.1 exchange on its own connection: the status and the body.
fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{extra}Content-Length: 0\r\n\r\n"
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
    (status, body.to_owned())
}

/// Each event published, as `(event, payload)`.
fn published(events: &[Value]) -> Vec<(String, Value)> {
    events
        .iter()
        .map(|event| {
            (
                event["event"].as_str().unwrap_or_default().to_owned(),
                event["payload"].clone(),
            )
        })
        .collect()
}

/// A desk on 127.0.0.1 answering one vouch request `{"holds": true}`, but only once `release`
/// is sent; `asked` is sent once the request has arrived.
fn held_desk() -> (SocketAddr, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
    let address = listener.local_addr().expect("the desk has an address");
    let (asked, arrived) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(ANSWER));
        let mut reader = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).is_err() {
                return;
            }
            let header = header.trim_end().to_ascii_lowercase();
            if header.is_empty() {
                break;
            }
            if let Some(value) = header.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let _ = asked.send(());
        let _ = released.recv_timeout(STARTUP);
        let body = json!({"holds": true}).to_string();
        let _ = write!(
            reader.into_inner(),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (address, arrived, release)
}

/// The environment document's `[seed] file`: while the desk holds the second entry's external
/// cause, the listener already answers, and only the first entry is in effect; once released,
/// the third follows. Each entry ran as its own actor and caller.
#[test]
fn the_listener_answers_before_the_seed_finishes_and_each_entry_runs_in_order_as_its_caller() {
    let directory = scratch();
    let (desk, asked, release) = held_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            {
                "command": VOUCH,
                "input": {"voucher_id": "v-1", "role_id": "admin"},
                "actor": VISITOR,
                "request_id": "seed-2",
            },
            grant("auditor", "south", "seed-3"),
        ])
        .to_string(),
    );
    let document = environment(
        &directory,
        "memory",
        Some(desk),
        &format!("\n[seed]\nfile = \"{seed}\"\n"),
    );
    let mut served = serve(&["--environment", &document]);
    asked
        .recv_timeout(ANSWER)
        .expect("the seed's second entry asks the desk");
    let held = published(&served.events(&[]));
    assert_eq!(
        held,
        vec![(
            "vault.access.RoleGranted".to_owned(),
            json!({"role_id": "admin", "tenant": "north"})
        )],
        "the listener answers while the seed is held at its second entry"
    );
    release.send(()).expect("the desk is released");
    let ended = served.ended();
    assert!(
        ended.contains("3 sent, 3 applied, 0 already there, 0 refused"),
        "{ended}"
    );
    assert_eq!(
        published(&served.events(&[])),
        vec![
            (
                "vault.access.RoleGranted".to_owned(),
                json!({"role_id": "admin", "tenant": "north"})
            ),
            (
                "vault.access.Vouched".to_owned(),
                json!({"voucher_id": "v-1"})
            ),
            (
                "vault.access.RoleGranted".to_owned(),
                json!({"role_id": "auditor", "tenant": "south"})
            ),
        ]
    );
    let stderr = served.stop();
    assert!(
        stderr.iter().all(|line| !line.contains("refused:")),
        "{stderr:#?}"
    );
}

/// The counts of the run at `address`'s `/__spindle/metrics`: `sent`, `applied`,
/// `already_there`, `refused`.
fn counted(address: SocketAddr) -> [u64; 4] {
    let (status, text) = exchange(address, "GET", "/__spindle/metrics", &[]);
    assert_eq!(status, 200, "{text}");
    ["sent", "applied", "already_there", "refused"].map(|counted| {
        let series = format!("spindle_seed_commands_total{{counted=\"{counted}\"}} ");
        text.lines()
            .find_map(|line| line.strip_prefix(&series))
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("no series {series}in {text}"))
    })
}

/// Run twice on one state: the second run applies nothing new, and every entry counts as already
/// there, the `existing_instance:` duplicate as much as a request id an earlier entry carried;
/// none counts as refused. The counts are in `/__spindle/metrics` and the end line.
#[test]
fn a_second_run_on_one_state_applies_nothing_new() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            grant("auditor", "north", "seed-2"),
            grant("admin", "north", "seed-1"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "sqlite", None, "");
    let arguments = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--answer-default",
        "--observe",
    ];

    let mut first = serve(&arguments);
    let ended = first.ended();
    assert!(
        ended.contains("3 sent, 2 applied, 1 already there, 0 refused"),
        "{ended}"
    );
    assert!(
        ended.contains(&seed),
        "the end line names the file: {ended}"
    );
    assert_eq!(counted(first.address), [3, 2, 1, 0]);
    assert_eq!(first.events(&[]).len(), 2);
    first.stop();

    let mut second = serve(&arguments);
    let ended = second.ended();
    assert!(
        ended.contains("3 sent, 0 applied, 3 already there, 0 refused"),
        "{ended}"
    );
    assert_eq!(counted(second.address), [3, 0, 3, 0]);
    let stderr = second.stop();
    assert!(
        stderr.iter().all(|line| !line.contains("refused:")),
        "an entry already there is no refusal: {stderr:#?}"
    );
}

/// An entry refused for any other reason, a declared error or a kernel refusal, is logged with its
/// command, request id and error name; the entries after it still run and the run keeps serving.
#[test]
fn a_refused_entry_is_logged_and_the_rest_still_run() {
    let directory = scratch();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-retire-ghost"},
            {"command": GRANT, "input": {"role_id": "admin", "title": "Admin"}, "actor": VISITOR,
             "request_id": "seed-visitor-grant"},
            grant("admin", "north", "seed-admin"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--seed",
        &seed,
        "--answer-default",
        "--observe",
    ]);
    let ended = served.ended();
    assert!(
        ended.contains("3 sent, 1 applied, 0 already there, 2 refused"),
        "{ended}"
    );
    assert_eq!(counted(served.address), [3, 1, 0, 2]);
    assert_eq!(
        published(&served.events(&[])),
        vec![(
            "vault.access.RoleGranted".to_owned(),
            json!({"role_id": "admin", "tenant": "north"})
        )],
        "the run keeps serving after the refusals"
    );
    let stderr = served.stop();
    let missing = stderr
        .iter()
        .find(|line| line.contains("seed-retire-ghost"))
        .unwrap_or_else(|| panic!("the refusal is logged: {stderr:#?}"));
    assert!(
        missing.contains("entry 1")
            && missing.contains(RETIRE)
            && missing.contains("refused: vault.access.RoleMissing"),
        "{missing}"
    );
    let visitor = stderr
        .iter()
        .find(|line| line.contains("seed-visitor-grant"))
        .unwrap_or_else(|| panic!("the refusal is logged: {stderr:#?}"));
    assert!(
        visitor.contains("entry 2")
            && visitor.contains(GRANT)
            && visitor.contains("refused: not_granted"),
        "{visitor}"
    );
}

/// On a trusted-caller run the seed needs no proxy secret: it does not come through the listener,
/// and its entries run as the file's actor and caller.
#[test]
fn a_trusted_caller_run_seeds_without_the_proxy_secret() {
    let directory = scratch();
    let secret = write(&directory, "proxy-secret", &format!("{SECRET}\n"));
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "west", "seed-1")]).to_string(),
    );
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--seed",
        &seed,
        "--answer-default",
        "--trusted-caller-header",
        HEADER,
        "--trusted-caller-secret",
        &secret,
    ]);
    let ended = served.ended();
    assert!(
        ended.contains("1 sent, 1 applied, 0 already there, 0 refused"),
        "{ended}"
    );
    let (status, _) = exchange(served.address, "GET", "/api/events", &[]);
    assert_eq!(
        status, 403,
        "the listener still refuses a request without the secret"
    );
    assert_eq!(
        published(&served.events(&[(SECRET_HEADER, SECRET)])),
        vec![(
            "vault.access.RoleGranted".to_owned(),
            json!({"role_id": "admin", "tenant": "west"})
        )]
    );
}

/// A seed file that does not parse, or names a command, field or actor shape the specification
/// does not declare, exits 1 before listening, naming the file and the entry. `--seed` wins over
/// the document's `[seed] file`, and a run that serves no HTTP refuses a seed.
#[test]
fn a_seed_file_that_does_not_parse_or_names_what_the_specification_does_not_declare_exits_1() {
    let directory = scratch();
    let document = environment(&directory, "memory", None, "");
    let operator = |command: &str, input: Value| {
        json!({"command": command, "input": input, "actor": OPERATOR,
               "caller": {"tenant": "north"}, "request_id": "r-2"})
    };
    let good = grant("admin", "north", "r-1");
    let cases: Vec<(&str, String, &str)> = vec![
        ("not JSON", "[{\"command\": ".to_owned(), "line 1"),
        ("not an array", json!({"entries": []}).to_string(), "array"),
        (
            "an unknown key",
            json!([good, {"command": GRANT, "request_id": "r-2", "inputs": {}}]).to_string(),
            "entry 2",
        ),
        (
            "no request id",
            json!([good, {"command": GRANT, "input": {}}]).to_string(),
            "entry 2",
        ),
        (
            "an empty request id",
            json!([good, {"command": GRANT, "input": {}, "request_id": ""}]).to_string(),
            "entry 2",
        ),
        (
            "an undeclared command",
            json!([good, operator("vault.access.Forge", json!({}))]).to_string(),
            "vault.access.Forge",
        ),
        (
            "an undeclared field",
            json!([good, operator(GRANT, json!({"role_id": "a", "title": "A", "rank": 1}))])
                .to_string(),
            "rank",
        ),
        (
            "a field of another type",
            json!([good, operator(GRANT, json!({"role_id": 7, "title": "A"}))]).to_string(),
            "role_id",
        ),
        (
            "an undeclared actor",
            json!([good, {"command": GRANT, "input": {"role_id": "a", "title": "A"}, "actor": "vault.access.Ghost",
                          "request_id": "r-2"}])
            .to_string(),
            "vault.access.Ghost",
        ),
        (
            "an undeclared caller attribute",
            json!([good, {"command": GRANT, "input": {"role_id": "a", "title": "A"}, "actor": OPERATOR,
                          "caller": {"tenant": "north", "badge": 3}, "request_id": "r-2"}])
            .to_string(),
            "entry 2",
        ),
        (
            "a required caller attribute left out",
            json!([good, {"command": GRANT, "input": {"role_id": "a", "title": "A"}, "actor": OPERATOR,
                          "request_id": "r-2"}])
            .to_string(),
            "entry 2",
        ),
        (
            "a caller without an actor",
            json!([good, {"command": GRANT, "input": {"role_id": "a", "title": "A"}, "caller": {"tenant": "north"},
                          "request_id": "r-2"}])
            .to_string(),
            "entry 2",
        ),
        (
            "one request id for two different entries",
            json!([good, grant("auditor", "north", "r-1")]).to_string(),
            "entry 2",
        ),
    ];
    for (case, text, named) in cases {
        let seed = write(&directory, "seed.json", &text);
        let stopped = start(
            SPEC,
            &[
                "--environment",
                &document,
                "--seed",
                &seed,
                "--answer-default",
            ],
        );
        let Err((code, stdout, stderr)) = stopped else {
            panic!("{case}: run listened");
        };
        assert_eq!(code, Some(1), "{case}: {stderr}");
        assert!(
            stdout.is_empty(),
            "{case}: nothing is printed on stdout: {stdout}"
        );
        assert!(
            stderr.contains(&seed) && stderr.contains(named),
            "{case}: the refusal names the file and {named:?}: {stderr}"
        );
    }

    let missing = directory.path().join("absent.json");
    let missing = missing.to_str().expect("a UTF-8 path");
    let named = environment(
        &directory,
        "memory",
        None,
        &format!("\n[seed]\nfile = \"{missing}\"\n"),
    );
    let Err((code, _, stderr)) = start(SPEC, &["--environment", &named, "--answer-default"]) else {
        panic!("a document naming a missing seed file listened");
    };
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains(missing), "{stderr}");
    let seed = write(&directory, "seed.json", &json!([good]).to_string());
    let mut served = start(
        SPEC,
        &["--environment", &named, "--seed", &seed, "--answer-default"],
    )
    .unwrap_or_else(|stopped| panic!("--seed wins over the document: {stopped:?}"));
    let ended = served.ended();
    assert!(ended.contains(&seed), "{ended}");

    let Err((code, _, stderr)) = start(
        IN_PROCESS,
        &[
            "--environment",
            "examples/environments/laptop.toml",
            "--seed",
            &seed,
        ],
    ) else {
        panic!("a run reading stdin listened");
    };
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("--seed"), "{stderr}");
}
