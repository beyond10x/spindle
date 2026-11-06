//! `b10x-spindle deploy`: the control process as the binary runs it, with the observe UI it
//! carries.
//!
//! The process a test starts is held by a guard that stops it when dropped, so a failing
//! assertion never leaves it, or the instances it launched, running; every wait has a hard bound.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

/// How long `deploy` may take to place, launch and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take.
const ANSWER: Duration = Duration::from_secs(10);
/// How long `deploy` may take to stop its instances and exit after SIGTERM.
const EXIT: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

/// Stops the process with SIGTERM when dropped, and SIGKILL if it does not exit in time.
struct Running(Child);

impl Running {
    fn terminate(&mut self) -> Option<std::process::ExitStatus> {
        let pid = rustix::process::Pid::from_child(&self.0);
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        let deadline = std::time::Instant::now() + EXIT;
        while std::time::Instant::now() < deadline {
            if let Ok(Some(status)) = self.0.try_wait() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.0.kill();
        self.0.wait().ok();
        None
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            self.terminate();
        }
    }
}

fn get(address: SocketAddr, path: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(address).expect("deploy listens");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        address.port()
    )
    .expect("the request is sent");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("the answer is read");
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a head");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    (status, raw[split + 4..].to_vec())
}

#[test]
fn the_ui_is_served_from_the_binary() {
    let dist = root().join("ui/dist");
    let index = std::fs::read_to_string(dist.join("index.html")).unwrap_or_else(|error| {
        panic!(
            "ui/dist/index.html: {error}; the UI is built before cargo (task check:ui, pnpm build \
             in ui/)"
        )
    });
    let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(tmp.path())
        .env_clear()
        .env("PATH", "")
        .env("TMPDIR", std::env::temp_dir())
        .arg("deploy")
        .arg(root().join("examples/rally/ess"))
        .arg("--environment")
        .arg(root().join("examples/environments/laptop-processes.toml"))
        .arg("--placement")
        .arg(root().join("examples/placements/rally-processes.toml"))
        .arg("--port")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("deploy starts");
    let mut running = Running(child);
    let stdout = running.0.stdout.take().expect("stdout");
    let (lines, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                return;
            }
        }
    });
    let first = received.recv_timeout(STARTUP).unwrap_or_else(|_| {
        let status = running.terminate();
        let mut stderr = String::new();
        if let Some(mut pipe) = running.0.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        panic!("deploy printed nothing within {STARTUP:?} ({status:?}):\n{stderr}")
    });
    let printed: Value = serde_json::from_str(&first).expect("deploy prints JSON");
    let address: SocketAddr = printed["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("no address in {printed}"));
    let url = printed["url"].as_str().expect("a URL");
    let token = url
        .split_once("?t=")
        .map(|(_, token)| token.to_owned())
        .expect("the URL carries the token");
    assert_eq!(url, format!("http://{address}/?t={token}"));

    let (status, body) = get(address, &format!("/?t={token}"));
    assert_eq!(status, 200);
    let served = String::from_utf8(body).expect("the index is UTF-8");
    let mut assets = Vec::new();
    for reference in index.split("\"./assets/").skip(1) {
        let name = reference.split('"').next().expect("a closing quote");
        assets.push(name.to_owned());
    }
    assert!(!assets.is_empty(), "the built index names no asset");
    let mut expected = index.clone();
    for name in &assets {
        expected = expected.replace(
            &format!("\"./assets/{name}\""),
            &format!("\"./assets/{name}?t={token}\""),
        );
    }
    assert_eq!(
        served, expected,
        "the built index.html, its assets carrying the token"
    );
    for name in &assets {
        let (status, body) = get(address, &format!("/assets/{name}?t={token}"));
        assert_eq!(status, 200, "{name}");
        assert_eq!(
            body,
            std::fs::read(dist.join("assets").join(name)).expect("the asset"),
            "{name}"
        );
        let (status, _) = get(address, &format!("/assets/{name}"));
        assert!((400..500).contains(&status), "{name} without the token");
    }
    let (status, _) = get(address, "/");
    assert!((400..500).contains(&status), "the index without the token");

    let status = running.terminate().expect("deploy exits on SIGTERM");
    assert_eq!(status.code(), Some(143), "{status:?}");
}
