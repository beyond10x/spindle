//! Adversary cases for `run` serving `form`- and `query`-encoded command requests.
//!
//! The authority is the server ESS 0.87.0 synthesises for the binding (`ess-synth`,
//! `rust/http/bound.rs`): `flat_object` form-decodes each pair and reads it with `flat_value`,
//! which makes an `Integer` text of exact decimal digits a JSON number spelt as given; the
//! input decoder then reads that number with `json::integer_at`, which takes
//! `spelling.parse::<i64>()` (so `0042` is 42 and `-0` is 0) and refuses a magnitude past 64 bits.
//!
//! `tests/fixtures/adversary-form-query`: `FileNotebook` on `POST /notebooks/{id}` read from a
//! form body (an `Integer`, a `Boolean`, an enum, an `Optional<Integer>`, an `Optional<String>`, a
//! `List<Integer>` and a `String` renamed `shelfMark` on the wire), `CloseNotebook` on
//! `DELETE /notebooks/{id}` read from the query string with `unknown: ignore`, and
//! `OpenNotebooks` on `GET /notebooks`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const LAPTOP: &str = "examples/environments/laptop.toml";
const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-form-query";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/adversary-form-query-http.yaml";
const FORM: &str = "application/x-www-form-urlencoded";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(["--http", BINDING])
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

struct Reply {
    status: u16,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text)
            .unwrap_or_else(|error| panic!("{error}: {} {:?}", self.status, self.text))
    }

    fn payload(&self) -> Value {
        self.json()["published"][0]["payload"].clone()
    }
}

fn send(
    address: SocketAddr,
    method: &str,
    target: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> Reply {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let mut head =
        format!("{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n");
    if let Some(content_type) = content_type {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut request = head.into_bytes();
    request.extend_from_slice(body);
    stream.write_all(&request).expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, text) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    Reply {
        status,
        text: text.to_owned(),
    }
}

fn file(address: SocketAddr, id: &str, body: &str) -> Reply {
    send(
        address,
        "POST",
        &format!("/notebooks/{id}"),
        Some(FORM),
        body.as_bytes(),
    )
}

fn events(address: SocketAddr) -> usize {
    let reply = send(address, "GET", "/api/events", None, b"");
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.json()["events"]
        .as_array()
        .unwrap_or_else(|| panic!("events: {}", reply.text))
        .len()
}

fn rows(address: SocketAddr) -> Value {
    let reply = send(address, "GET", "/notebooks", None, b"");
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.json()["rows"].clone()
}

const REST: &str = "ruled=true&colour=red&shelfMark=A1";

/// ESS reads `pages=0042` as the JSON number `0042`, and its decoder as
/// `"0042".parse::<i64>()`, 42: the command runs. Spindle's `read` hands the text to
/// `serde_json::Number`, which refuses a leading zero, so it stays a string and the kernel
/// refuses it.
#[test]
fn an_integer_with_a_leading_zero_is_read_as_ess_reads_it() {
    let served = serve();
    let reply = file(served.address, "n1", &format!("pages=0042&{REST}"));
    assert_eq!(
        reply.status, 201,
        "ESS's served application reads `0042` as the integer 42: {}",
        reply.text
    );
    assert_eq!(
        reply.payload()["pages"].as_i64(),
        Some(42),
        "{}",
        reply.text
    );
}

/// ESS reads `pages=-0` as `"-0".parse::<i64>()`, 0, and publishes the integer `0`.
#[test]
fn negative_zero_is_the_integer_zero() {
    let served = serve();
    let reply = file(served.address, "n1", &format!("pages=-0&{REST}"));
    assert_eq!(reply.status, 201, "{}", reply.text);
    let pages = &reply.payload()["pages"];
    assert_eq!(
        pages.to_string(),
        "0",
        "ESS publishes the i64 it decoded, `0`, not the spelling `-0`: {}",
        reply.text
    );
    assert_eq!(rows(served.address), json!([{"id": "n1", "pages": 0}]));
}

/// ESS refuses an `Integer` past 64 bits (`integer_at`: "a magnitude past 64 bits. Refused
/// rather than rounded"), `400`, and nothing is written.
#[test]
fn an_integer_past_64_bits_is_refused_400_and_writes_nothing() {
    let served = serve();
    let before = events(served.address);
    for pages in ["99999999999999999999", "9223372036854775808"] {
        let reply = file(served.address, "n1", &format!("pages={pages}&{REST}"));
        assert_eq!(reply.status, 400, "pages={pages}: {}", reply.text);
    }
    assert_eq!(
        events(served.address),
        before,
        "a refused form writes nothing"
    );
    assert_eq!(rows(served.address), json!([]));
}

/// The flat edges ESS draws: `Boolean` only `true`/`false`, an enum only its variants, an
/// `Optional<Integer>` or `List<Integer>` value that is the empty text refused, a scalar key
/// twice refused, `;` no separator, a malformed escape in a known value refused. Each `400`,
/// nothing written.
#[test]
fn flat_values_ess_refuses_are_refused_400_and_write_nothing() {
    let served = serve();
    let before = events(served.address);
    for body in [
        "pages=1&ruled=TRUE&colour=red&shelfMark=A1",
        "pages=1&ruled=1&colour=red&shelfMark=A1",
        "pages=1&ruled=&colour=red&shelfMark=A1",
        "pages=1&ruled=true&colour=green&shelfMark=A1",
        "pages=1&ruled=true&colour=red&shelfMark=A1&shelf=",
        "pages=1&ruled=true&colour=red&shelfMark=A1&tags=",
        "pages=1&ruled=true&colour=red&shelfMark=A1&tags=1&tags=two",
        "pages=1&pages=1&ruled=true&colour=red&shelfMark=A1",
        "pages=1;ruled=true&colour=red&shelfMark=A1",
        "pages=1&ruled=true&colour=red&shelfMark=A%2",
        "pages=1&ruled=true&colour=red&shelfMark=%C3",
        "pages=1&ruled=true&colour=red&shelf_mark=A1",
        "pages=1&ruled=true&colour=red",
        "pages=1.0&ruled=true&colour=red&shelfMark=A1",
        "pages=+1&ruled=true&colour=red&shelfMark=A1",
    ] {
        let reply = file(served.address, "n1", body);
        assert_eq!(reply.status, 400, "{body}: {}", reply.text);
    }
    assert_eq!(
        events(served.address),
        before,
        "a refused form writes nothing"
    );
    assert_eq!(rows(served.address), json!([]));
}

/// What ESS accepts at the edges: an absent `Optional` omitted, an `Optional<String>` given as
/// the empty text (or as a bare key) the empty string, an absent `List` the empty array, a
/// repeated `List` key its elements in order, a renamed field under its wire name.
#[test]
fn flat_values_ess_accepts_are_read_as_ess_reads_them() {
    let served = serve();
    let reply = file(served.address, "n1", &format!("pages=3&{REST}"));
    assert_eq!(reply.status, 201, "{}", reply.text);
    let payload = reply.payload();
    assert_eq!(payload["tags"], json!([]), "{}", reply.text);
    assert_eq!(payload["shelf_mark"], "A1", "{}", reply.text);
    assert!(payload["shelf"].is_null(), "{}", reply.text);

    let reply = file(
        served.address,
        "n2",
        &format!("pages=3&{REST}&label=&tags=2&tags=1&shelf=7"),
    );
    assert_eq!(reply.status, 201, "{}", reply.text);
    let payload = reply.payload();
    assert_eq!(payload["label"], "", "{}", reply.text);
    assert_eq!(payload["tags"], json!([2, 1]), "{}", reply.text);
    assert_eq!(payload["shelf"], 7, "{}", reply.text);

    let reply = file(served.address, "n3", &format!("label&pages=3&{REST}"));
    assert_eq!(
        reply.status, 201,
        "a bare key is the empty text: {}",
        reply.text
    );
    assert_eq!(reply.payload()["label"], "", "{}", reply.text);
}

/// `unknown: ignore` on a query route: an undeclared key, a key that does not decode and the
/// key the path carries are all dropped, and the path's id is the one acted on.
#[test]
fn unknown_ignore_drops_unknown_and_carried_query_keys() {
    let served = serve();
    assert_eq!(
        file(served.address, "n1", &format!("pages=3&{REST}")).status,
        201
    );
    assert_eq!(
        file(served.address, "n2", &format!("pages=4&{REST}")).status,
        201
    );
    let reply = send(
        served.address,
        "DELETE",
        "/notebooks/n1?id=n2&colour=red&%ZZ=1&reason=worn",
        None,
        b"",
    );
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.payload(),
        json!({"id": "n1", "reason": "worn"}),
        "{}",
        reply.text
    );
    assert_eq!(rows(served.address), json!([{"id": "n2", "pages": 4}]));
}
