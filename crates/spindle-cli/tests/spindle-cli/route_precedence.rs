//! `run` serving an `ess-http/1` binding in which two routes of one method match one request path
//! (`story:route-literal-segment-precedence`).
//!
//! The specification `tests/fixtures/route-precedence/spec` declares commands asked after a thing
//! by id, after the modules and after the actions. `precedence-http.yaml` binds them to
//! `GET /things/{id}`, declared first, `GET /things/modules` and `GET /things/actions`;
//! `reversed-http.yaml` declares the same routes in the opposite order. A literal segment answers
//! before a path variable whatever the order (`ESS-HTTP-011`, admitted from ESS 0.89.0 for a
//! nested pair). `tilde-http.yaml` binds the literals as `~modules` and `~actions`, which sort
//! after `{id}` in the order ESS hands the routes over. `method-http.yaml` binds `GET /things/{id}` and `POST /things/modules`, and no
//! other route whose template carries `/things/modules`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/route-precedence/spec";
const PRECEDENCE: &str = "crates/spindle-cli/tests/fixtures/route-precedence/precedence-http.yaml";
const REVERSED: &str = "crates/spindle-cli/tests/fixtures/route-precedence/reversed-http.yaml";
const TILDE: &str = "crates/spindle-cli/tests/fixtures/route-precedence/tilde-http.yaml";
const METHOD: &str = "crates/spindle-cli/tests/fixtures/route-precedence/method-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

/// `run` on the fixture with `binding`. The fixture declares no entity, so it keeps no state.
fn serve(binding: &str) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(["--http", binding])
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
    Served { child, address }
}

/// One answer: its status, its head and its JSON body.
struct Reply {
    status: u16,
    head: String,
    body: Value,
}

impl Reply {
    /// The value of the header `name`, compared case-insensitively.
    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then_some(value.trim())
        })
    }
}

/// One HTTP/1.1 exchange without a body on its own connection.
fn exchange(address: SocketAddr, method: &str, path: &str) -> Reply {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
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
    Reply {
        status,
        head: head.to_owned(),
        body,
    }
}

/// `GET /things/<literal>modules`, `GET /things/<literal>actions` and `GET /things/42` each
/// answer their own command's outcome, `literal` being what the binding's literal segments start
/// with.
fn each_route_answers_its_own_requests(binding: &str, literal: &str) {
    let served = serve(binding);
    for (path, outcome, published) in [
        (
            format!("/things/{literal}modules"),
            "modules-asked",
            json!({"event": "catalogue.things.ModulesAsked", "payload": {}}),
        ),
        (
            format!("/things/{literal}actions"),
            "actions-asked",
            json!({"event": "catalogue.things.ActionsAsked", "payload": {}}),
        ),
        (
            "/things/42".to_owned(),
            "thing-asked",
            json!({"event": "catalogue.things.ThingAsked", "payload": {"id": "42"}}),
        ),
    ] {
        let reply = exchange(served.address, "GET", &path);
        assert_eq!(
            (reply.status, &reply.body),
            (200, &json!({"outcome": outcome, "published": [published]})),
            "{binding}: GET {path} answers the route bound to it\n{}",
            reply.head
        );
    }
}

#[test]
fn a_literal_segment_wins_over_a_path_variable() {
    each_route_answers_its_own_requests(PRECEDENCE, "");
}

#[test]
fn declaration_order_does_not_change_the_match() {
    each_route_answers_its_own_requests(REVERSED, "");
}

/// ESS hands the routes over sorted by their path text, in which `{` sorts after every letter and
/// digit, so a literal of those comes first by accident. `~`, path text RFC 3986 admits in a
/// literal, sorts after `{`: here the by-id route comes first, and the literal still answers.
#[test]
fn a_literal_sorting_after_a_variable_still_wins() {
    each_route_answers_its_own_requests(TILDE, "~");
}

#[test]
fn a_method_no_matching_route_serves_answers_405() {
    let served = serve(METHOD);
    let reply = exchange(served.address, "DELETE", "/things/modules");
    assert_eq!(reply.status, 405, "{}\n{}", reply.head, reply.body);
    let allowed = reply
        .header("allow")
        .unwrap_or_else(|| panic!("a 405 names what the path answers in Allow: {}", reply.head));
    let mut allowed = allowed.split(',').map(str::trim).collect::<Vec<_>>();
    allowed.sort_unstable();
    assert_eq!(
        allowed,
        ["GET", "POST"],
        "the methods of both routes whose templates carry the path: {}",
        reply.head
    );
}
