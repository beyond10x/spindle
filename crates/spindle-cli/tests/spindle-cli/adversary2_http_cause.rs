//! Adversary pass 2 on `story:external-cause-bound-to-http-call` (wave 18 U3), against the fix
//! round 594016f7: the userinfo refusal, the reply limit, the timeout bound and the observed
//! `call` / `unanswered` entries, each driven at its edges.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_capability::{Environment, EnvironmentError};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/http-cause";
const SERVED_SPEC: &str = "crates/spindle-cli/tests/fixtures/served-http-cause";
const ASK: &str = "probe.approval.Ask";
const CAPABILITY: &str = "external.probe.approval.Ask.approved";
const SECRET: &str = "SPINDLE_ADVERSARY2_APPROVAL_TOKEN";
const TOKEN: &str = "adversary2-token-value";
const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);
const LIMIT: usize = 64 * 1024;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory")
}

#[derive(Debug, Clone)]
struct Received {
    line: String,
    headers: Vec<(String, String)>,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }
}

type Replies = Arc<dyn Fn(usize, &mut TcpStream) + Send + Sync>;

struct Desk {
    address: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Desk {
    fn on(listener: TcpListener, replies: Replies) -> Self {
        let address = listener.local_addr().expect("an address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&received);
        let count = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let recorded = Arc::clone(&recorded);
                let replies = Arc::clone(&replies);
                let count = Arc::clone(&count);
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(ANSWER));
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.is_empty() {
                        return;
                    }
                    let mut headers = Vec::new();
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err() {
                            return;
                        }
                        let header = header.trim_end();
                        if header.is_empty() {
                            break;
                        }
                        if let Some((name, value)) = header.split_once(':') {
                            headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
                        }
                    }
                    let length = headers
                        .iter()
                        .find(|(name, _)| name == "content-length")
                        .and_then(|(_, value)| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    let mut body = vec![0; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    recorded.lock().expect("the record").push(Received {
                        line: line.trim_end().to_owned(),
                        headers,
                    });
                    let n = count.fetch_add(1, Ordering::SeqCst);
                    let mut stream = reader.into_inner();
                    replies(n, &mut stream);
                });
            }
        });
        Self { address, received }
    }

    fn start(replies: Replies) -> Self {
        Self::on(
            TcpListener::bind("127.0.0.1:0").expect("the desk binds"),
            replies,
        )
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the record").clone()
    }
}

fn json_reply(stream: &mut TcpStream, status: u16, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Answer\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// `{"holds": true}` padded with leading spaces to exactly `length` bytes.
fn padded(length: usize) -> String {
    let tail = r#"{"holds": true}"#;
    format!("{}{tail}", " ".repeat(length - tail.len()))
}

fn document(http: &str, served: bool) -> String {
    let ingress = if served {
        "\"ingress.network\" = [\"http-loopback\"]\n\"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\nchannel = [\"in-process\"]\nprojection = [\"in-process\"]\n"
    } else {
        ""
    };
    format!(
        "format = \"spindle-environment/1\"\nname = \"approvals\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"memory\"]\n\"store.ephemeral\" = [\"memory\"]\n{ingress}\
         \"{CAPABILITY}\" = [\"approval-desk\"]\n\"external.*\" = [\"operator-prompt\"]\n\n{http}"
    )
}

fn table(url: &str, timeout: &str) -> String {
    format!(
        "[http.approval-desk]\nmethod = \"POST\"\nurl = \"{url}\"\n{timeout}\nsecret_env = \"{SECRET}\"\n"
    )
}

fn write(directory: &tempfile::TempDir, text: &str) -> String {
    let path = directory.path().join("approvals.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

fn ask(id: &str) -> String {
    format!(r#"{{"command": "{ASK}", "input": {{"request_id": "{id}", "amount": 7}}}}"#) + "\n"
}

fn run(environment: &str, lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", environment])
        .env(SECRET, TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(lines.as_bytes())
        .expect("the lines are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

// ---- 1. a credential in the document never reaches an error -----------------------------------

/// The fix round: a URL carrying userinfo is refused "naming the table and never the URL". The
/// same document read with a mistake on the URL's own line (an inline `[http]` table, a TOML form
/// the format accepts) is refused by the TOML reader before that check, and its error quotes the
/// line, credential included.
#[test]
fn a_mistake_beside_a_credential_url_never_prints_the_credential() {
    let url = "http://desk-user:desk-password@127.0.0.1:9/approvals";
    let mut leaked = Vec::new();
    for (mistake, row) in [
        (
            "a negative timeout",
            format!("approval-desk = {{ method = \"POST\", url = \"{url}\", timeout_ms = -1 }}"),
        ),
        (
            "no timeout",
            format!("approval-desk = {{ method = \"POST\", url = \"{url}\" }}"),
        ),
        (
            "a misspelt key",
            format!(
                "approval-desk = {{ method = \"POST\", url = \"{url}\", timeout = 100, timeout_ms = 100 }}"
            ),
        ),
    ] {
        let directory = tempdir();
        let environment = write(&directory, &document(&format!("[http]\n{row}\n"), false));
        let output = run(&environment, "");
        assert_ne!(output.status.code(), Some(0), "{mistake}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stderr.contains("desk-password") || stdout.contains("desk-password") {
            leaked.push(format!("{mistake}:\n{stderr}"));
        }
    }
    assert!(
        leaked.is_empty(),
        "the refusal prints the URL's password:\n{}",
        leaked.join("\n")
    );
}

// ---- 2. the timeout bound at each edge ---------------------------------------------------------

#[test]
fn the_timeout_bound_holds_at_each_edge() {
    let parse = |timeout: &str| {
        Environment::parse(&document(&table("http://127.0.0.1:1/", timeout), false))
    };
    for accepted in ["timeout_ms = 1", "timeout_ms = 30000"] {
        let parsed = parse(accepted);
        assert!(parsed.is_ok(), "{accepted}: {parsed:?}");
    }
    for (refused, quoted) in [
        ("timeout_ms = 0", "0"),
        ("timeout_ms = 30001", "30001"),
        ("timeout_ms = 9223372036854775807", "9223372036854775807"),
    ] {
        match parse(refused) {
            Err(error @ EnvironmentError::HttpTimeout { .. }) => assert!(
                error
                    .to_string()
                    .contains(&format!("timeout_ms is {quoted};")),
                "{refused}: {error}"
            ),
            other => panic!("{refused} is refused as a timeout: {other:?}"),
        }
    }
    for unread in [
        "timeout_ms = -1",
        "timeout_ms = 1.5",
        "timeout_ms = \"100\"",
        "timeout_ms = 1e3",
        "",
    ] {
        assert!(
            matches!(parse(unread), Err(EnvironmentError::Parse(_))),
            "{unread:?} is not read: {:?}",
            parse(unread)
        );
    }
}

// ---- 3. every userinfo form, and no other `@` -------------------------------------------------

#[test]
fn every_userinfo_form_is_refused_and_an_at_sign_elsewhere_is_not() {
    let parse = |url: &str| Environment::parse(&document(&table(url, "timeout_ms = 100"), false));
    for url in [
        "http://desk-user%40corp:desk-password@127.0.0.1:1/",
        "http://@127.0.0.1:1/",
        "http://:desk-password@127.0.0.1:1/",
        "http://desk-password@[::1]:1/",
        "Http://desk-user:desk-password@127.0.0.1/",
        "http://desk-user:desk-password@b\u{fc}cher.example/",
        "http://desk-user:desk-password@127.0.0.1:1",
        "http://a@b@127.0.0.1:1/",
    ] {
        match parse(url) {
            Err(error @ EnvironmentError::UrlUserinfo { .. }) => assert!(
                !error.to_string().contains("desk-password") && !error.to_string().contains(url),
                "{url}: {error}"
            ),
            other => panic!("{url} is refused as userinfo: {other:?}"),
        }
    }
    for url in [
        "http://127.0.0.1:1/a@b",
        "http://127.0.0.1:1/?to=a@b",
        "http://127.0.0.1:1/#a@b",
        "http://127.0.0.1:1/a%40b",
        "http://[::1]:1/",
        "http://[fe80::1%25eth0]:1/",
    ] {
        let parsed = parse(url);
        assert!(parsed.is_ok(), "{url}: {parsed:?}");
    }
}

// ---- 4. Host from host[:port]: IPv6, an uppercase scheme, a query ------------------------------

#[test]
fn an_ipv6_desk_named_with_an_uppercase_scheme_is_sent_host_and_port_only() {
    let Ok(listener) = TcpListener::bind("[::1]:0") else {
        eprintln!("no IPv6 loopback here; nothing to test");
        return;
    };
    let desk = Desk::on(
        listener,
        Arc::new(|_, stream: &mut TcpStream| json_reply(stream, 200, r#"{"holds": true}"#)),
    );
    let port = desk.address.port();
    let directory = tempdir();
    let environment = write(
        &directory,
        &document(
            &table(
                &format!("HTTP://[::1]:{port}/approvals?to=a@b"),
                "timeout_ms = 5000",
            ),
            false,
        ),
    );
    let output = run(&environment, &ask("r1"));
    let results = results(&output);
    assert_eq!(results.len(), 1, "{output:?}");
    assert_eq!(results[0]["outcome"], "approved", "{results:?}");
    let received = desk.received();
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(received[0].line, "POST /approvals?to=a@b HTTP/1.1");
    assert_eq!(
        received[0].header("host"),
        Some(format!("[::1]:{port}").as_str()),
        "{received:?}"
    );
}

// ---- 5. the reply limit, its edges, and replies that are no answer -----------------------------

#[test]
fn a_reply_at_the_limit_answers_and_every_longer_or_malformed_reply_fails() {
    let elsewhere = Desk::start(Arc::new(|_, stream: &mut TcpStream| {
        json_reply(stream, 200, r#"{"holds": true}"#)
    }));
    let away = elsewhere.address;
    let desk = Desk::start(Arc::new(move |n, stream: &mut TcpStream| match n {
        0 => json_reply(stream, 200, &padded(LIMIT)),
        1 => json_reply(stream, 200, &padded(LIMIT + 1)),
        2 => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            );
            let chunk = " ".repeat(1000);
            for _ in 0..70 {
                let _ = write!(stream, "{:x}\r\n{chunk}\r\n", chunk.len());
            }
            let tail = r#"{"holds": true}"#;
            let _ = write!(stream, "{:x}\r\n{tail}\r\n0\r\n\r\n", tail.len());
        }
        3 => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            );
            for piece in [r#"{"hol"#, r#"ds": "#, "true}"] {
                let _ = write!(stream, "{:x}\r\n{piece}\r\n", piece.len());
            }
            let _ = write!(stream, "0\r\n\r\n");
        }
        4 => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n{{\"holds\": true}}"
            );
        }
        5 => {
            let _ = write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: http://{away}/approvals\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
        6 => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 3\r\nConnection: close\r\n\r\nyes"
            );
        }
        7 => {
            let _ = write!(stream, "HTTP/1.0 200 OK\r\n\r\n{{\"holds\": true}}");
        }
        _ => {
            let _ = write!(stream, "HTTP/1.0 200 OK\r\n\r\n{}", " ".repeat(LIMIT + 1));
        }
    }));
    let directory = tempdir();
    let environment = write(
        &directory,
        &document(
            &table(
                &format!("http://{}/approvals", desk.address),
                "timeout_ms = 5000",
            ),
            false,
        ),
    );
    let lines: String = (0..9).map(|n| ask(&format!("r{n}"))).collect();
    let output = run(&environment, &lines);
    let results = results(&output);
    assert_eq!(results.len(), 9, "{output:?}");
    let expected = [
        ("exactly the limit", true),
        ("one byte over", false),
        ("chunked over", false),
        ("chunked under", true),
        ("a Content-Length longer than the body", false),
        ("a cross-host redirect", false),
        ("a body that is not JSON", false),
        ("HTTP/1.0 read to close", true),
        ("HTTP/1.0 over the limit", false),
    ];
    let mut wrong = Vec::new();
    for ((case, answers), result) in expected.iter().zip(&results) {
        let answered = result["outcome"] == "approved";
        let failed = result["failure"]["kind"] == "undecided";
        if (*answers && !answered) || (!*answers && !failed) {
            wrong.push(format!("{case}: {result}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert_eq!(desk.received().len(), 9);
    assert!(
        elsewhere.received().is_empty(),
        "a redirect is not followed"
    );
    let over = results[1]["failure"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(over.contains("65536"), "{over}");
}

// ---- 6. a reply dripped slower than the timeout -------------------------------------------------

#[test]
fn a_reply_dripped_a_byte_at_a_time_fails_at_the_timeout() {
    let desk = Desk::start(Arc::new(|_, stream: &mut TcpStream| {
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 400\r\nConnection: close\r\n\r\n"
        );
        for _ in 0..400 {
            if stream.write_all(b" ").is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }));
    let directory = tempdir();
    let environment = write(
        &directory,
        &document(
            &table(
                &format!("http://{}/approvals", desk.address),
                "timeout_ms = 1000",
            ),
            false,
        ),
    );
    let started = Instant::now();
    let output = run(&environment, &ask("r1"));
    let took = started.elapsed();
    let results = results(&output);
    assert_eq!(results.len(), 1, "{output:?}");
    assert_eq!(results[0]["failure"]["kind"], "undecided", "{results:?}");
    assert!(
        results[0]["failure"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("1000 ms")),
        "{results:?}"
    );
    assert!(took < Duration::from_secs(20), "took {took:?}");
}

// ---- 7. every call recorded once ---------------------------------------------------------------

struct Served {
    child: Child,
    address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(environment: &str) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            SERVED_SPEC,
            "--environment",
            environment,
            "--observe",
        ])
        .env(SECRET, TOKEN)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver.recv_timeout(STARTUP).expect("a first line");
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str()?.parse().ok())
        .unwrap_or_else(|| panic!("no address in {line:?}"));
    Served { child, address }
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer");
    let answer = String::from_utf8(answer).expect("UTF-8");
    let (head, body) = answer.split_once("\r\n\r\n").expect("a head");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    (status, body.to_owned())
}

fn route() -> (String, String) {
    let ir = spindle_spec::load(&root().join(SERVED_SPEC))
        .expect("the fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["probe".to_owned()], None)
        .expect("the surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == ASK)
        .expect("a route for Ask");
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// The desk fails its first call and approves every later one. Across a failed call, an answered
/// one, the same request again, and a reset whose world answers the cause the other way, each call
/// the desk received is one `externals` entry and one count, `call` where it answered and
/// `unanswered` where it failed, and the world's table never answers a cause bound to a call.
#[test]
fn every_call_is_listed_and_counted_once_across_failures_repeats_and_a_reset() {
    let desk = Desk::start(Arc::new(|n, stream: &mut TcpStream| {
        if n == 0 {
            json_reply(stream, 503, r#"{"holds": true}"#);
        } else {
            json_reply(stream, 200, r#"{"holds": true}"#);
        }
    }));
    let directory = tempdir();
    let environment = write(
        &directory,
        &document(
            &table(
                &format!("http://{}/approvals", desk.address),
                "timeout_ms = 5000",
            ),
            true,
        ),
    );
    let served = serve(&environment);
    let (method, path) = route();
    let body = |id: &str| json!({"request_id": id, "amount": 7}).to_string();
    let (first, answer) = exchange(served.address, &method, &path, &body("r1"));
    assert_eq!(first, 501, "{answer}");
    let (second, answer) = exchange(served.address, &method, &path, &body("r1"));
    assert!((200..300).contains(&second), "{second} {answer}");
    let _ = exchange(served.address, &method, &path, &body("r1"));
    let world = json!({"decisions": {ASK: {"approved": false}}}).to_string();
    let (reset, answer) = exchange(served.address, "POST", "/api/reset", &world);
    assert!((200..300).contains(&reset), "{reset} {answer}");
    let (fourth, answer) = exchange(served.address, &method, &path, &body("r2"));
    assert!(
        (200..300).contains(&fourth) && answer.contains("approved"),
        "the call answers before the world's table: {fourth} {answer}"
    );

    let calls = desk.received().len();
    let (_, log) = exchange(served.address, "GET", "/__spindle/requests", "");
    let log: Value = serde_json::from_str(&log).expect("the log is JSON");
    let externals: Vec<&Value> = log["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .filter(|record| record["operation"] == ASK)
        .flat_map(|record| record["externals"].as_array().into_iter().flatten())
        .collect();
    assert_eq!(externals.len(), calls, "{log}");
    let sources = |source: &str| {
        externals
            .iter()
            .filter(|external| external["source"] == source)
            .count()
    };
    assert_eq!(sources("unanswered"), 1, "{log}");
    assert_eq!(sources("call"), calls - 1, "{log}");

    let (_, metrics) = exchange(served.address, "GET", "/__spindle/metrics", "");
    let counted = |source: &str| -> u64 {
        metrics
            .lines()
            .filter(|line| {
                line.starts_with("spindle_external_causes_total{")
                    && line.contains(&format!("source=\"{source}\""))
            })
            .filter_map(|line| line.rsplit(' ').next()?.parse::<u64>().ok())
            .sum()
    };
    assert_eq!(counted("unanswered"), 1, "{metrics}");
    assert_eq!(counted("call"), (calls - 1) as u64, "{metrics}");
    assert_eq!(counted("world"), 0, "{metrics}");
}
