//! `run --observe` serving the observe panel at `GET /__spindle/ui` (`story:ingress-observe-panel`):
//! one HTML page that holds its script and style inline, loads nothing else and may be framed by
//! any page.
//!
//! The page is `ui/dist-panel/panel.html`, built by `pnpm build:panel` and embedded into
//! `b10x-spindle` by its `build.rs`; `task check` builds it before cargo. A surface given no page
//! answers the route `503`, naming the page.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";
const PANEL: &str = "/__spindle/ui";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A running `b10x-spindle run --observe` serving HTTP; stopped when dropped.
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

/// `run --observe` on the ingress fixture and laptop.
fn observed() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            INGRESS,
            "--environment",
            LAPTOP,
            "--observe",
            "--state",
        ])
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
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} is not JSON: {error}")
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

/// One answer: its status, its headers by lower-case name, and its body text.
pub(crate) struct Answered {
    status: u16,
    headers: BTreeMap<String, String>,
    body: String,
}

/// `GET path` on its own connection; `observe` reads the panel with it too.
pub(crate) fn get(address: SocketAddr, path: &str) -> Answered {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
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
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Answered {
        status,
        headers,
        body: body.to_owned(),
    }
}

/// Every value an attribute `name=` of `page` gives, quoted or not.
fn values<'a>(page: &'a str, name: &str) -> Vec<&'a str> {
    let mut values = Vec::new();
    let mut rest = page;
    while let Some(at) = rest.find(name) {
        rest = &rest[at + name.len()..];
        let value = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let inner = &rest[1..];
                &inner[..inner.find(quote).unwrap_or(inner.len())]
            }
            _ => {
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(rest.len());
                &rest[..end]
            }
        };
        values.push(value);
    }
    values
}

/// The directives of a `Content-Security-Policy`, by name.
fn directives(policy: &str) -> BTreeMap<String, String> {
    policy
        .split(';')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .map(|directive| {
            let (name, sources) = directive.split_once(' ').unwrap_or((directive, ""));
            (name.to_ascii_lowercase(), sources.trim().to_owned())
        })
        .collect()
}

#[test]
fn the_panel_is_one_self_contained_page() {
    let served = observed();
    let answered = get(served.address, PANEL);
    assert_eq!(answered.status, 200, "{}", answered.body);
    let media = answered
        .headers
        .get("content-type")
        .expect("the page has a Content-Type");
    assert!(media.starts_with("text/html"), "{media}");
    let page = &answered.body;
    assert!(
        page.contains("<script"),
        "the page holds its script: {page}"
    );
    for name in ["src=", "href="] {
        for value in values(page, name) {
            assert!(
                value.starts_with('#') || value.starts_with("data:"),
                "{name}{value:?} loads something beside the page"
            );
        }
    }
    let lowered = page.to_ascii_lowercase();
    assert!(
        !lowered.contains("<link rel=\"stylesheet\"") && !lowered.contains("rel=stylesheet"),
        "the page links a stylesheet"
    );
}

#[test]
fn the_panel_may_be_framed() {
    let served = observed();
    let answered = get(served.address, PANEL);
    assert_eq!(answered.status, 200, "{}", answered.body);
    assert!(
        !answered.headers.contains_key("x-frame-options"),
        "{:?}",
        answered.headers
    );
    let policy = answered
        .headers
        .get("content-security-policy")
        .expect("the page has a Content-Security-Policy");
    let directives = directives(policy);
    assert!(!directives.contains_key("frame-ancestors"), "{policy}");
    assert_eq!(
        directives.get("connect-src").map(String::as_str),
        Some("'self'"),
        "{policy}"
    );
    for kind in ["script-src", "style-src"] {
        let sources = directives
            .get(kind)
            .unwrap_or_else(|| panic!("{kind} is declared: {policy}"));
        assert!(sources.contains("'unsafe-inline'"), "{policy}");
    }
}

#[test]
fn without_the_built_page_the_panel_answers_503() {
    let ir = spindle_spec::load(&root().join(INGRESS))
        .expect("the ingress fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["board".to_owned()], None)
        .expect("the board surface is served")
        .observing(8);
    let listener = spindle_ingress::Listener::bind(0, surface).expect("the listener binds");
    let answered = get(listener.local_addr(), PANEL);
    assert_eq!(answered.status, 503, "{}", answered.body);
    let body: Value = serde_json::from_str(&answered.body)
        .unwrap_or_else(|error| panic!("{error}: {}", answered.body));
    let refused = body["refused"].as_str().unwrap_or_default();
    assert!(refused.contains("ui/dist-panel/panel.html"), "{body}");
}

#[test]
fn only_get_at_the_panel_path_answers_the_page() {
    let ir = spindle_spec::load(&root().join(INGRESS))
        .expect("the ingress fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["board".to_owned()], None)
        .expect("the board surface is served")
        .observing(8)
        .with_panel("<!doctype html><title>panel</title>");
    let listener = spindle_ingress::Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let answered = get(address, PANEL);
    assert_eq!(answered.status, 200, "{}", answered.body);
    assert_eq!(answered.body, "<!doctype html><title>panel</title>");
    for below in ["/__spindle/ui/", "/__spindle/ui/panel.js"] {
        let answered = get(address, below);
        assert_eq!(answered.status, 404, "{below}: {}", answered.body);
    }
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "POST {PANEL} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    )
    .expect("the request is written");
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is read before the timeout");
    assert!(answer.starts_with("HTTP/1.1 405"), "{answer}");
    assert!(
        answer.to_ascii_lowercase().contains("\r\nallow: get\r\n"),
        "{answer}"
    );
}
