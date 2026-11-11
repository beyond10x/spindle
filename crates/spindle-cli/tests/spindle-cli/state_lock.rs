//! `story:state-directory-lock`: `run --state <path>` holds an exclusive lock beside its state
//! until it exits, so a second `run` on the same state exits 1 before opening anything, naming the
//! path and the process that holds it. The operating system releases the lock with the process,
//! a SIGKILL included.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A `b10x-spindle` process, killed and waited on when dropped.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Drop for Running {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn signal(&self, signal: rustix::process::Signal) {
        rustix::process::kill_process(rustix::process::Pid::from_child(&self.child), signal)
            .expect("the signal is sent");
    }

    fn exited(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + ANSWER;
        loop {
            if let Some(status) = self.child.try_wait().expect("waited on") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "run did not exit within {ANSWER:?}"
            );
            std::thread::sleep(POLL);
        }
    }
}

/// `run` serving HTTP with `--state directory`, once it printed the address it listens on.
fn serve(directory: &Path) -> (Running, SocketAddr) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", INGRESS, "--environment", LAPTOP, "--state"])
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let running = Running { child, stdin: None };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver
        .recv_timeout(STARTUP)
        .unwrap_or_else(|_| panic!("run printed no line within {STARTUP:?}"));
    let ready: serde_json::Value =
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("a JSON line: {line:?}"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("no address: {ready}"));
    (running, address)
}

/// `arguments` run to their end within [`STARTUP`], or the test fails: a second run that is not
/// refused keeps serving and never ends by itself.
fn finished(arguments: &[&std::ffi::OsStr]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let deadline = Instant::now() + STARTUP;
    loop {
        if child.try_wait().expect("waited on").is_some() {
            return child.wait_with_output().expect("the output is read");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("the output is read");
            panic!(
                "the second run did not exit within {STARTUP:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(POLL);
    }
}

/// A second `run` serving HTTP on `directory`.
fn second_serve(directory: &Path) -> Output {
    finished(&[
        "run".as_ref(),
        INGRESS.as_ref(),
        "--environment".as_ref(),
        LAPTOP.as_ref(),
        "--state".as_ref(),
        directory.as_os_str(),
    ])
}

/// `GET /api/model` on its own connection: the status.
fn status(address: SocketAddr) -> u16 {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    write!(
        stream,
        "GET /api/model HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is read");
    answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {answer:?}"))
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

#[test]
fn a_second_run_on_the_same_state_exits_1() {
    let scratch = scratch();
    let directory = scratch.path().join("state");
    let (_first, _) = serve(&directory);
    let second = second_serve(&directory);
    assert_eq!(second.status.code(), Some(1), "{second:?}");
    assert!(second.stdout.is_empty(), "nothing is served: {second:?}");
}

#[test]
fn the_refusal_names_the_holder() {
    let scratch = scratch();
    let directory = scratch.path().join("state");
    let (first, _) = serve(&directory);
    let second = second_serve(&directory);
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains(&directory.display().to_string()),
        "the directory is named: {stderr}"
    );
    assert!(
        stderr.contains(&format!("process {}", first.pid())),
        "the holder's process id {} is named: {stderr}",
        first.pid()
    );
}

#[test]
fn the_holder_keeps_serving() {
    let scratch = scratch();
    let directory = scratch.path().join("state");
    let (_first, address) = serve(&directory);
    assert_eq!(status(address), 200);
    let second = second_serve(&directory);
    assert_eq!(second.status.code(), Some(1), "{second:?}");
    assert_eq!(status(address), 200, "the first run still answers");
}

#[test]
fn the_lock_is_released_at_exit() {
    let scratch = scratch();
    let directory = scratch.path().join("state");
    let (mut first, _) = serve(&directory);
    first.signal(rustix::process::Signal::TERM);
    first.exited();
    let (_next, address) = serve(&directory);
    assert_eq!(status(address), 200);
}

#[test]
fn a_killed_holder_does_not_block() {
    let scratch = scratch();
    let directory = scratch.path().join("state");
    let (mut first, _) = serve(&directory);
    first.signal(rustix::process::Signal::KILL);
    first.exited();
    let (_next, address) = serve(&directory);
    assert_eq!(status(address), 200);
}

/// `run` reading stdin locks its state file itself: a second one given a hard link to the file the
/// first holds, a path that resolves to no other, exits 1 naming the path it was given.
#[test]
fn a_second_stdin_run_on_a_hard_link_to_the_held_state_file_exits_1() {
    let scratch = scratch();
    let file = scratch.path().join("state.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", "ess", "--environment", LAPTOP, "--state"])
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdin = child.stdin.take();
    let _first = Running { child, stdin };
    let lock = scratch.path().join("state.sqlite.lock");
    let deadline = Instant::now() + STARTUP;
    while std::fs::read_to_string(&lock).map_or(true, |written| written.trim().is_empty()) {
        assert!(
            Instant::now() < deadline,
            "{} is not written within {STARTUP:?}",
            lock.display()
        );
        std::thread::sleep(POLL);
    }
    let linked = scratch.path().join("linked.sqlite");
    std::fs::hard_link(&file, &linked).expect("the hard link is made");
    let second = finished(&[
        "run".as_ref(),
        "ess".as_ref(),
        "--environment".as_ref(),
        LAPTOP.as_ref(),
        "--state".as_ref(),
        linked.as_os_str(),
    ]);
    assert_eq!(second.status.code(), Some(1), "{second:?}");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains(&linked.display().to_string()) && stderr.contains("in use"),
        "the path given is named in use: {stderr}"
    );
}

/// `run` reading stdin takes the lock beside its state file: a second one on the same file exits
/// 1 while the first still reads.
#[test]
fn a_second_stdin_run_on_the_same_state_file_exits_1() {
    let scratch = scratch();
    let file = scratch.path().join("state.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", "ess", "--environment", LAPTOP, "--state"])
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdin = child.stdin.take();
    let first = Running { child, stdin };
    let deadline = Instant::now() + STARTUP;
    while !file.exists() {
        assert!(
            Instant::now() < deadline,
            "{} is not created within {STARTUP:?}",
            file.display()
        );
        std::thread::sleep(POLL);
    }
    let second = finished(&[
        "run".as_ref(),
        "ess".as_ref(),
        "--environment".as_ref(),
        LAPTOP.as_ref(),
        "--state".as_ref(),
        file.as_os_str(),
    ]);
    assert_eq!(second.status.code(), Some(1), "{second:?}");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains(&file.display().to_string()),
        "the file is named: {stderr}"
    );
    assert!(
        stderr.contains(&format!("process {}", first.pid())),
        "the holder's process id {} is named: {stderr}",
        first.pid()
    );
}
