//! Adversary cases for `story:identity-mint-types-and-fallback` over `run`'s HTTP surface.
//!
//! The fixture `tests/fixtures/adversary-w25-mint` declares one component, `yard`, reached over
//! the network: `yard.crates.Crate`, whose `Integer` identity the runtime mints and `RenameCrate`
//! takes back as input, and `yard.crates.Note`, whose bare `String` identity the runtime mints.
//! `POST /api/reset` with `{"mint": <seed>}` seeds the mint, and a world that holds a row the seed
//! draws first must still create (`Seeded::avoiding`, `Host::reset_to`: "a world reloaded with its
//! own mint seed creates without a collision").

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-w25-mint";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "yard";

const CRATE: &str = "yard.crates.Crate";
const NOTE: &str = "yard.crates.Note";
const OPEN_CRATE: &str = "yard.crates.OpenCrate";
const RENAME_CRATE: &str = "yard.crates.RenameCrate";
const OPEN_NOTE: &str = "yard.crates.OpenNote";
const ALL_CRATES: &str = "yard.crates.AllCrates";

/// The largest integer every JSON reader holds exactly: 2^53 - 1.
const JSON_SAFE: u64 = (1 << 53) - 1;

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

fn call(served: &Served, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(id);
    let body = body.map(Value::to_string).unwrap_or_default();
    let (status, answer) = exchange(served.address, &method, &path, &body);
    (status, json_of(&answer))
}

fn reset(served: &Served, world: &Value) -> (u16, Value) {
    let (status, answer) = exchange(served.address, "POST", "/api/reset", &world.to_string());
    (status, json_of(&answer))
}

/// Opens a crate named `name`: the status, the answer, and the raw body as sent on the wire.
fn open_crate(served: &Served, name: &str) -> (u16, Value, String) {
    let (method, path) = route(OPEN_CRATE);
    let (status, body) = exchange(
        served.address,
        &method,
        &path,
        &json!({"name": name}).to_string(),
    );
    (status, json_of(&body), body)
}

fn crate_id(answer: &Value) -> u64 {
    answer["published"][0]["payload"]["crate_id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a non-negative integer identity is published: {answer}"))
}

/// A minted `Integer` identity crosses HTTP as a JSON number a double holds exactly, and the
/// next request that sends it back as `crate_id` addresses that crate.
#[test]
fn adv_a_minted_integer_identity_crosses_http_and_comes_back_as_input() {
    let served = serve();
    let (status, answer, body) = open_crate(&served, "first");
    assert!((200..300).contains(&status), "{status} {answer}");
    let identity = crate_id(&answer);
    assert!(identity <= JSON_SAFE, "{identity} is past 2^53 - 1: {body}");
    assert!(
        body.contains(&format!("\"crate_id\":{identity}")),
        "the identity is a bare JSON number on the wire, not text: {body}"
    );

    let (status, answer) = call(
        &served,
        RENAME_CRATE,
        Some(&json!({"crate_id": identity, "name": "second"})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "renamed", "{answer}");

    let (status, rows) = call(&served, ALL_CRATES, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(
        rows["rows"],
        json!([{"crate_id": identity, "name": "second", "state": "Open"}]),
        "the view lists the crate under its minted identity"
    );
}

/// A world seeded with the bare `String` identity its own mint seed draws first: the next
/// creation still creates, under another identity.
#[test]
fn adv_a_seeded_mint_does_not_mint_a_seeded_bare_string_identity() {
    let served = serve();
    assert_eq!(reset(&served, &json!({"mint": 7})).0, 200);
    let (status, answer) = call(&served, OPEN_NOTE, Some(&json!({"body": "captured"})));
    assert!((200..300).contains(&status), "{status} {answer}");
    let drawn = answer["published"][0]["payload"]["note_id"].clone();
    assert!(drawn.is_string(), "{answer}");

    let world = json!({
        "rows": [{"entity": NOTE, "identity": drawn, "state": "Open", "fields": {"body": "captured"}}],
        "mint": 7,
    });
    let (status, answer) = reset(&served, &world);
    assert_eq!(status, 200, "{answer}");
    let (status, answer) = call(&served, OPEN_NOTE, Some(&json!({"body": "next"})));
    assert!(
        (200..300).contains(&status),
        "the first creation after a world seeded with the note the mint draws first: {status} \
         {answer}"
    );
    assert_ne!(
        answer["published"][0]["payload"]["note_id"], drawn,
        "{answer}"
    );
}

/// A world seeded with the `Integer` identity its own mint seed draws first: the next creation
/// must create under another identity, as it does for a `Uuid` or `String` identity
/// (`Seeded::avoiding` skips only UUIDs a seeded text holds or ends in), and neither fail on the
/// seeded row nor write a second crate under the seeded crate's identity.
#[test]
fn adv_a_seeded_mint_does_not_mint_a_seeded_integer_identity() {
    let served = serve();
    assert_eq!(reset(&served, &json!({"mint": 7})).0, 200);
    let (status, answer, _) = open_crate(&served, "captured");
    assert!((200..300).contains(&status), "{status} {answer}");
    let drawn = crate_id(&answer);

    let world = json!({
        "rows": [{"entity": CRATE, "identity": drawn, "state": "Open", "fields": {"name": "captured"}}],
        "mint": 7,
    });
    let (status, answer) = reset(&served, &world);
    assert_eq!(status, 200, "{answer}");
    let (status, answer, _) = open_crate(&served, "next");
    assert!(
        (200..300).contains(&status),
        "the first creation after a world seeded with the crate the mint draws first: {status} \
         {answer}"
    );
    let (status, rows) = call(&served, ALL_CRATES, None);
    assert_eq!(status, 200, "{rows}");
    let listed = rows["rows"].as_array().expect("rows").clone();
    // By the number, not its spelling: `42` and `42.0` are one `Integer`.
    let identities: std::collections::BTreeSet<String> = listed
        .iter()
        .map(|row| format!("{:?}", row["crate_id"].as_f64()))
        .collect();
    assert_eq!(
        identities.len(),
        listed.len(),
        "no two crates share an identity: {rows}"
    );
    assert_ne!(
        crate_id(&answer),
        drawn,
        "a minted identity is never a seeded one: {answer}"
    );
}

/// A world row under an `Integer` identity is the crate a later request names by that integer:
/// `RenameCrate` with the seeded `crate_id` renames it, and the view lists one crate.
#[test]
fn adv_a_seeded_integer_row_is_addressed_by_its_integer_input() {
    let served = serve();
    let world = json!({
        "rows": [{"entity": CRATE, "identity": 42, "state": "Open", "fields": {"name": "seeded"}}],
    });
    let (status, answer) = reset(&served, &world);
    assert_eq!(status, 200, "{answer}");
    let (status, answer) = call(
        &served,
        RENAME_CRATE,
        Some(&json!({"crate_id": 42, "name": "renamed"})),
    );
    assert_eq!(
        answer["outcome"], "renamed",
        "the seeded crate 42 is the crate `crate_id: 42` names: {status} {answer}"
    );
    let (status, rows) = call(&served, ALL_CRATES, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(
        rows["rows"],
        json!([{"crate_id": 42, "name": "renamed", "state": "Open"}]),
        "one crate, under the integer it was seeded with"
    );
}
