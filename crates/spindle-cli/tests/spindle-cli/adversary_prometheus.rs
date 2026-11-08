//! Adversary cases for `GET /__spindle/metrics` (`story:ingress-prometheus-metrics`): every
//! histogram series read as the Prometheus text format 0.0.4 states it (its `+Inf` bucket equal
//! to its `_count`), the series an unregistered method or an unmatched path may add, and requests
//! answered concurrently, each counted once.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const INGRESS_HTTP: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);

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

fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", INGRESS, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(["--http", INGRESS_HTTP, "--observe"])
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
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    let ready: serde_json::Value = serde_json::from_str(&line).expect("a JSON line");
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("no address: {ready}"));
    Served {
        child,
        address,
        _state: state,
    }
}

/// `method path` with `body` on its own connection: the status and the body as text.
fn text(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8(answer).expect("UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    (status, body.to_owned())
}

fn scrape(served: &Served) -> String {
    let (status, body) = text(served.address, "GET", "/__spindle/metrics", "");
    assert_eq!(status, 200, "{body}");
    body
}

/// Every sample line of `exposition`, as its series (name and label text) and its value.
fn lines(exposition: &str) -> BTreeMap<String, f64> {
    exposition
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let (series, value) = line.rsplit_once(' ').expect("a sample has a value");
            (
                series.to_owned(),
                value.parse().unwrap_or_else(|_| panic!("a number: {line}")),
            )
        })
        .collect()
}

/// The Prometheus text format 0.0.4 ("Histograms and summaries"): "A histogram must have a
/// bucket with `{le="+Inf"}`. Its value must be identical to the value of `x_count`." Every
/// `+Inf` bucket therefore needs an `x_count` with the same labels and value; a client library's
/// text parser (`expfmt`) reads a missing `_count` as a sample count of 0.
#[test]
fn adversary_every_histogram_series_has_a_count_equal_to_its_inf_bucket() {
    let served = serve();
    for task in ["t1", "t2"] {
        let (status, body) = text(
            served.address,
            "PUT",
            &format!("/tasks/{task}"),
            r#"{"title": "Sweep"}"#,
        );
        assert_eq!(status, 201, "{body}");
    }
    let exposition = scrape(&served);
    let samples = lines(&exposition);
    let mut checked = 0;
    for (series, value) in &samples {
        let Some((name, labels)) = series.split_once("_bucket") else {
            continue;
        };
        let Some(labels) = labels
            .strip_suffix("le=\"+Inf\"}")
            .map(|rest| rest.trim_end_matches(','))
        else {
            continue;
        };
        let count = if labels == "{" {
            format!("{name}_count")
        } else {
            format!("{name}_count{labels}}}")
        };
        assert_eq!(
            samples.get(&count),
            Some(value),
            "{series} {value} has no {count} of the same value:\n{exposition}"
        );
        checked += 1;
    }
    assert!(
        checked > 1,
        "the command histograms are among the series checked:\n{exposition}"
    );
}

/// A method RFC 9110 does not register, and paths no route matches, add no series beyond one per
/// method class and status class: forty unmatched paths and three invented methods leave the
/// request family with a bounded number of series, and no method label but the registered ones
/// and `other`.
#[test]
fn adversary_methods_and_unmatched_paths_mint_no_series() {
    let served = serve();
    for index in 0..40 {
        let (status, body) = text(served.address, "GET", &format!("/nowhere/{index}"), "");
        assert_eq!(status, 404, "{body}");
    }
    for method in ["BREW", "PROPFIND", "get"] {
        let _ = text(served.address, method, "/tasks", "");
        let _ = text(served.address, method, "/nowhere/else", "");
    }
    let exposition = scrape(&served);
    let requests = lines(&exposition)
        .into_iter()
        .filter(|(series, _)| series.starts_with("spindle_http_requests_total{"))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        requests.values().sum::<f64>(),
        46.0,
        "every request is counted:\n{exposition}"
    );
    for series in requests.keys() {
        assert!(
            ["GET", "other"]
                .iter()
                .any(|method| series.contains(&format!("method=\"{method}\""))),
            "{series}"
        );
        assert!(!series.contains("nowhere"), "{series}");
    }
    assert!(
        requests.len() <= 4,
        "a bounded number of series: {requests:#?}"
    );
}

/// Requests answered at once are each counted once, in the request family, the duration
/// histogram, the command counters and the rows.
#[test]
fn adversary_concurrent_requests_are_each_counted_once() {
    const MANY: usize = 32;
    let served = serve();
    let address = served.address;
    let threads = (0..MANY)
        .map(|index| {
            std::thread::spawn(move || {
                text(
                    address,
                    "PUT",
                    &format!("/tasks/c{index}"),
                    r#"{"title": "Sweep"}"#,
                )
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        let (status, body) = thread.join().expect("the request thread");
        assert_eq!(status, 201, "{body}");
    }
    let exposition = scrape(&served);
    let samples = lines(&exposition);
    let opened = "spindle_http_requests_total{method=\"PUT\",route=\"/tasks/{task_id}\",operation=\"tasks.board.OpenTask\",kind=\"command\",class=\"served\",status_class=\"2xx\"}";
    let expected = MANY as f64;
    assert_eq!(samples.get(opened), Some(&expected), "{exposition}");
    assert_eq!(
        samples.get("spindle_http_request_duration_seconds_count"),
        Some(&expected),
        "{exposition}"
    );
    assert_eq!(
        samples.get("spindle_commands_total{command=\"tasks.board.OpenTask\",outcome=\"opened\"}"),
        Some(&expected),
        "{exposition}"
    );
    assert_eq!(
        samples.get("spindle_entity_rows{entity=\"tasks.board.Task\"}"),
        Some(&expected),
        "{exposition}"
    );
}
