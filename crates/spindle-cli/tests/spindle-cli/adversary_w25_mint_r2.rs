//! Adversary cases, round 2, for `story:identity-mint-types-and-fallback` over `run`'s HTTP
//! surface: a minted `Integer` identity a bin already holds is drawn again, at most one draw
//! more than the bins held (`creation_identity`).
//!
//! The fixture `tests/fixtures/adversary-w25-mint-r2` declares one component, `shelf`, reached
//! over the network: `bins.store.Bin` under an `Integer` identity, opened by `OpenBin` under a
//! minted one and by `OpenBinAs` under the one the request sends, else a minted one.
//! `POST /api/reset` with `{"mint": <seed>}` seeds the mint, so a caller learns the integers the
//! seed draws by opening bins once and resetting.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-w25-mint-r2";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "shelf";

const OPEN_BIN: &str = "bins.store.OpenBin";
const OPEN_BIN_AS: &str = "bins.store.OpenBinAs";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == COMPONENT)
        .expect("the fixture declares the component");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
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
        .args([
            "run",
            SPEC,
            "--environment",
            LAPTOP,
            "--answer-default",
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

fn exchange(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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
    (status, body.to_owned())
}

fn json_of(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"))
}

/// Calls `id` with `body` as written on the wire.
fn call_raw(served: &Served, id: &str, body: &str) -> (u16, Value) {
    let (method, path) = route(id);
    let (status, answer) = exchange(served.address, &method, &path, body);
    (status, json_of(&answer))
}

fn reset(served: &Served, world: &Value) {
    let (status, answer) = exchange(served.address, "POST", "/api/reset", &world.to_string());
    assert_eq!(status, 200, "the reset loads: {answer}");
}

fn bin_id(answer: &Value) -> u64 {
    answer["published"][0]["payload"]["bin_id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a non-negative integer identity is published: {answer}"))
}

/// The first `count` integers the mint seeded with 7 draws, as the bins it opens publish them.
fn seeded_draws(served: &Served, count: usize) -> Vec<u64> {
    reset(served, &json!({"mint": 7}));
    let drawn = (0..count)
        .map(|_| {
            let (status, answer) =
                call_raw(served, OPEN_BIN, &json!({"name": "probe"}).to_string());
            assert!((200..300).contains(&status), "{status} {answer}");
            bin_id(&answer)
        })
        .collect();
    reset(served, &json!({"mint": 7}));
    drawn
}

/// Eight bins a request opened under the eight integers the seeded mint draws next: ESS's
/// interpreter draws one candidate beyond the held population and opens the next bin under the
/// ninth; this records what `run` answers instead.
#[test]
fn adv_r2_eight_held_bins_and_the_ninth_minted_creation_over_http() {
    let served = serve();
    let drawn = seeded_draws(&served, 9);
    for held in &drawn[..8] {
        let (status, answer) = call_raw(
            &served,
            OPEN_BIN_AS,
            &json!({"bin_id": held, "name": "held"}).to_string(),
        );
        assert!((200..300).contains(&status), "{status} {answer}");
    }
    let (status, answer) = call_raw(&served, OPEN_BIN, &json!({"name": "ninth"}).to_string());
    assert!(
        (200..300).contains(&status),
        "eight bins held, the interpreter opens the ninth under {}: run answered {status} {answer}",
        drawn[8]
    );
    assert_eq!(bin_id(&answer), drawn[8], "{answer}");
}
