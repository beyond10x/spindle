//! Adversary of wave 31 U7 (`story:event-egress-webhook`): the served `run`'s `[egress]` read
//! against the story's own words. Each event is posted at least once and never lost, in commit
//! order per component store; a webhook that answers `2xx` has the event; the pause between two
//! posts of one event is capped at 5 s; each component has a lane of its own; and a refused
//! `[egress]` names why.
//!
//! The webhook is a stub on 127.0.0.1 run by this test: it records each request with when it
//! arrived and answers as the test tells it to (a status, a long body, a delay, silence, a closed
//! connection or a redirect). No request leaves loopback.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::egress::{BINDING, SPEC, Served, exchange, listening, root};

const OPENED: &str = "tasks.board.TaskOpened";
const CLOSED: &str = "tasks.board.TaskClosed";
const RECORDED: &str = "tasks.ledger.ClosureRecorded";

/// One component whose `OpenNote` publishes `NoteOpened` and `NoteCounted` in one write.
const NOTES: &str = "crates/spindle-cli/tests/fixtures/adversary-w31-u7";
const NOTES_HTTP: &str = "crates/spindle-cli/tests/fixtures/adversary-w31-u7-http.yaml";
const NOTE_OPENED: &str = "notes.pad.NoteOpened";
const NOTE_COUNTED: &str = "notes.pad.NoteCounted";

/// What the stub does with one request.
#[derive(Clone, Debug)]
enum Reply {
    /// Answers the status with no body.
    Status(u16),
    /// Answers the status with a body of this many bytes.
    WithBody(u16, usize),
    /// Waits, then answers the status with no body.
    After(Duration, u16),
    /// Answers nothing for this long, then closes the connection.
    Silent(Duration),
    /// Closes the connection without an answer.
    Closed,
    /// Answers `307` to this path on the stub.
    Redirect(&'static str),
}

impl Reply {
    /// The status the stub answers, `None` for none.
    fn status(&self) -> Option<u16> {
        match self {
            Self::Status(status) | Self::WithBody(status, _) | Self::After(_, status) => {
                Some(*status)
            }
            Self::Redirect(_) => Some(307),
            Self::Silent(_) | Self::Closed => None,
        }
    }
}

/// One request the stub was sent.
#[derive(Clone, Debug)]
struct Got {
    at: Instant,
    line: String,
    body: Value,
    status: Option<u16>,
}

type Decide = dyn Fn(usize, &Value) -> Reply + Send + Sync;

/// A webhook on 127.0.0.1 answering the `n`th request (from 0) and its body as `decide` says.
struct Stub {
    address: SocketAddr,
    got: Arc<Mutex<Vec<Got>>>,
    decide: Arc<Mutex<Arc<Decide>>>,
}

impl Stub {
    fn start(decide: impl Fn(usize, &Value) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the stub binds");
        let address = listener.local_addr().expect("the stub has an address");
        let got = Arc::new(Mutex::new(Vec::new()));
        let decide: Arc<Mutex<Arc<Decide>>> = Arc::new(Mutex::new(Arc::new(decide)));
        let (recorded, deciding) = (Arc::clone(&got), Arc::clone(&decide));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (recorded, deciding) = (Arc::clone(&recorded), Arc::clone(&deciding));
                std::thread::spawn(move || serve(stream, &recorded, &deciding));
            }
        });
        Self {
            address,
            got,
            decide,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/events", self.address)
    }

    /// Answers as `decide` says from now on.
    fn answer(&self, decide: impl Fn(usize, &Value) -> Reply + Send + Sync + 'static) {
        *self.decide.lock().expect("the stub's rule") = Arc::new(decide);
    }

    fn got(&self) -> Vec<Got> {
        self.got.lock().expect("the stub's record").clone()
    }

    /// The bodies the stub answered `2xx`, in arrival order.
    fn accepted(&self) -> Vec<Value> {
        self.got()
            .into_iter()
            .filter(|got| {
                got.status
                    .is_some_and(|status| (200..300).contains(&status))
            })
            .map(|got| got.body)
            .collect()
    }

    /// Waits up to `limit` until `done` holds for what the stub got.
    fn wait(&self, what: &str, limit: Duration, done: impl Fn(&[Got]) -> bool) -> Vec<Got> {
        let deadline = Instant::now() + limit;
        loop {
            let got = self.got();
            if done(&got) {
                return got;
            }
            assert!(
                Instant::now() < deadline,
                "{what} within {limit:?}; the stub got: {got:#?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits until the stub accepted `count` bodies.
    fn wait_accepted(&self, count: usize, limit: Duration) -> Vec<Value> {
        self.wait(&format!("{count} bodies accepted"), limit, |got| {
            got.iter()
                .filter(|got| {
                    got.status
                        .is_some_and(|status| (200..300).contains(&status))
                })
                .count()
                >= count
        });
        self.accepted()
    }
}

/// Reads one request from `stream`, records it, and answers as `decide` says.
fn serve(stream: TcpStream, got: &Mutex<Vec<Got>>, decide: &Mutex<Arc<Decide>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let reply = {
        let rule = Arc::clone(&decide.lock().expect("the stub's rule"));
        let mut record = got.lock().expect("the stub's record");
        let reply = rule(record.len(), &body);
        record.push(Got {
            at: Instant::now(),
            line: line.trim_end().to_owned(),
            body,
            status: reply.status(),
        });
        reply
    };
    let mut stream = reader.into_inner();
    let head = |status: u16, length: usize| {
        format!("HTTP/1.1 {status} Answer\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
    };
    let _ = match reply {
        Reply::Status(status) => stream.write_all(head(status, 0).as_bytes()),
        Reply::WithBody(status, length) => stream
            .write_all(head(status, length).as_bytes())
            .and_then(|()| stream.write_all(&vec![b'a'; length])),
        Reply::After(pause, status) => {
            std::thread::sleep(pause);
            stream.write_all(head(status, 0).as_bytes())
        }
        Reply::Silent(pause) => {
            std::thread::sleep(pause);
            Ok(())
        }
        Reply::Closed => Ok(()),
        Reply::Redirect(path) => {
            let address = stream.local_addr().expect("the stub's address");
            stream.write_all(
                format!(
                    "HTTP/1.1 307 Moved\r\nLocation: http://{address}{path}\r\nContent-Length: \
                     0\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
        }
    };
    let _ = stream.flush();
}

/// An environment keeping state in SQLite, posting each of `events` to the call `hook` at `url`
/// with `method` and `timeout_ms`.
fn environment(
    directory: &Path,
    url: &str,
    events: &[&str],
    method: &str,
    timeout_ms: u64,
) -> PathBuf {
    let egress: String = events
        .iter()
        .map(|event| format!("\"{event}\" = \"hook\"\n"))
        .collect();
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"egress\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"sqlite\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\n\
         channel = [\"in-process\"]\nprojection = [\"in-process\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"external.*\" = [\"operator-prompt\"]\n\n\
         [egress]\n{egress}\n\
         [http.hook]\nmethod = \"{method}\"\nurl = \"{url}\"\ntimeout_ms = {timeout_ms}\n"
    );
    let path = directory.join("egress.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path
}

/// `run` on `spec` served through `binding`, with `environment`, keeping its state in `state`.
fn run(spec: &str, binding: &str, environment: &Path, state: &Path, more: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .args(["run", spec, "--http", binding, "--environment"])
        .arg(environment)
        .arg("--state")
        .arg(state)
        .args(more);
    command
}

fn tasks(environment: &Path, state: &Path) -> Served {
    listening(run(SPEC, BINDING, environment, state, &[]))
}

fn open(served: &Served, id: &str) {
    let (status, answer) = exchange(
        served.address,
        "PUT",
        &format!("/tasks/{id}"),
        Some(&json!({"title": format!("task {id}")})),
    );
    assert_eq!(status, 201, "{answer}");
}

fn close(served: &Served, id: &str) {
    let (status, answer) = exchange(served.address, "POST", &format!("/tasks/{id}/close"), None);
    assert_eq!(status, 200, "{answer}");
}

/// The event and the `task_id` (or `note_id`) of each body.
fn named(bodies: &[Value]) -> Vec<(String, String)> {
    bodies
        .iter()
        .map(|body| {
            let fields = &body["fields"];
            let id = fields["task_id"]
                .as_str()
                .or_else(|| fields["note_id"].as_str())
                .unwrap_or_default();
            (
                body["event"].as_str().unwrap_or_default().to_owned(),
                id.to_owned(),
            )
        })
        .collect()
}

fn pair(event: &str, id: &str) -> (String, String) {
    (event.to_owned(), id.to_owned())
}

fn positions(bodies: &[Value]) -> Vec<u64> {
    bodies
        .iter()
        .map(|body| body["position"].as_u64().expect("a position"))
        .collect()
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory")
}

fn is_2xx(got: &Got) -> bool {
    got.status
        .is_some_and(|status| (200..300).contains(&status))
}

/// D7 A retries a webhook that answers non-2xx or does not answer; one that answers `200` has
/// the event, whatever body it adds. A reply body over 64 KiB makes the call count as failed, so
/// the event is posted again and again, every post accepted, and the lane is stuck on it.
#[test]
fn a_2xx_answer_with_a_long_body_is_not_posted_again() {
    let stub = Stub::start(|_, _| Reply::WithBody(200, 70 * 1024));
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 2000);
    let served = tasks(&environment, &directory.path().join("state"));
    open(&served, "t1");
    stub.wait_accepted(1, Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(2));
    let got = stub.got();
    assert_eq!(
        got.len(),
        1,
        "the webhook answered 200 to the first post, and was sent the event {} times: {:?}",
        got.len(),
        got.iter()
            .map(|got| (got.status, named(std::slice::from_ref(&got.body))))
            .collect::<Vec<_>>()
    );
}

/// The backoff is capped at 5 s (D7 A). The unit's own test turns its webhook to `200` 5 s after
/// the first post, before the doubling pause (0.1, 0.2, ... 3.2 s) ever reaches the cap, so it
/// passes with no cap at all. Here the webhook answers `503` to the first seven posts: the gap
/// before the eighth is the first one the cap bounds (5 s capped, 6.4 s uncapped).
#[test]
fn the_pause_between_two_posts_of_one_event_is_at_most_five_seconds() {
    let stub = Stub::start(|index, _| {
        if index < 7 {
            Reply::Status(503)
        } else {
            Reply::Status(200)
        }
    });
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 2000);
    let served = tasks(&environment, &directory.path().join("state"));
    open(&served, "t1");
    let got = stub.wait("the eighth post", Duration::from_secs(25), |got| {
        got.len() >= 8
    });
    let gaps: Vec<Duration> = got
        .windows(2)
        .map(|pair| pair[1].at.duration_since(pair[0].at))
        .collect();
    assert!(
        gaps.iter().all(|gap| *gap <= Duration::from_millis(5_500)),
        "every pause is capped at 5 s: {gaps:?}"
    );
    assert!(
        gaps.iter().any(|gap| *gap >= Duration::from_millis(4_500)),
        "the cap was reached: {gaps:?}"
    );
    assert_eq!(named(&stub.accepted()), [pair(OPENED, "t1")], "{got:#?}");
    drop(served);
}

/// A SIGKILL while the post is under way: the webhook got the event and never answered, so the
/// cursor was not saved, and the restart posts the event again.
#[test]
fn an_event_whose_post_was_under_way_at_a_kill_is_posted_after_the_restart() {
    let stub = Stub::start(|index, _| {
        if index == 0 {
            Reply::Silent(Duration::from_secs(3))
        } else {
            Reply::Status(200)
        }
    });
    let directory = scratch();
    let state = directory.path().join("state");
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 10_000);
    let served = tasks(&environment, &state);
    open(&served, "t1");
    stub.wait("the first post", Duration::from_secs(10), |got| {
        !got.is_empty()
    });
    served.kill();

    let _again = tasks(&environment, &state);
    let accepted = stub.wait_accepted(1, Duration::from_secs(10));
    assert_eq!(named(&accepted), [pair(OPENED, "t1")]);
}

/// A SIGKILL right after the webhook answered `200`, likely before the run's thread saved the
/// cursor: the event may be posted again, never lost, and never after a later one.
#[test]
fn a_kill_right_after_the_answer_loses_no_event_and_keeps_commit_order() {
    let stub = Stub::start(|_, _| Reply::Status(200));
    let directory = scratch();
    let state = directory.path().join("state");
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 2000);
    let served = tasks(&environment, &state);
    open(&served, "t1");
    stub.wait_accepted(1, Duration::from_secs(10));
    served.kill();

    let again = tasks(&environment, &state);
    open(&again, "t2");
    stub.wait("t2 accepted", Duration::from_secs(10), |got| {
        got.iter()
            .any(|got| is_2xx(got) && got.body["fields"]["task_id"] == "t2")
    });
    std::thread::sleep(Duration::from_millis(500));
    let accepted = named(&stub.accepted());
    let first_t2 = accepted
        .iter()
        .position(|named| named.1 == "t2")
        .expect("t2 was accepted");
    assert!(
        accepted[..first_t2].iter().all(|named| named.1 == "t1")
            && accepted[first_t2..].iter().all(|named| named.1 == "t2"),
        "t1, once or more, then t2 once: {accepted:?}"
    );
    assert!(first_t2 >= 1, "t1 is never lost: {accepted:?}");
    assert_eq!(accepted.len() - first_t2, 1, "t2 once: {accepted:?}");
}

/// Commits made while the webhook answers `503` are held back behind the first, then posted once
/// each in commit order with strictly rising positions.
#[test]
fn events_committed_during_an_outage_are_posted_once_each_in_commit_order() {
    let stub = Stub::start(|_, _| Reply::Status(503));
    let directory = scratch();
    let environment = environment(
        directory.path(),
        &stub.url(),
        &[OPENED, CLOSED],
        "POST",
        2000,
    );
    let served = tasks(&environment, &directory.path().join("state"));
    open(&served, "t1");
    open(&served, "t2");
    close(&served, "t1");
    open(&served, "t3");
    close(&served, "t2");
    stub.wait("two refused posts", Duration::from_secs(10), |got| {
        got.len() >= 2
    });
    let refused = stub.got();
    assert!(
        refused.iter().all(|got| got.body == refused[0].body),
        "only the first event is posted while it is refused: {refused:#?}"
    );
    stub.answer(|_, _| Reply::Status(200));

    let accepted = stub.wait_accepted(5, Duration::from_secs(15));
    std::thread::sleep(Duration::from_millis(500));
    let accepted_now = stub.accepted();
    assert_eq!(accepted_now.len(), 5, "{accepted_now:#?}");
    assert_eq!(
        named(&accepted),
        [
            pair(OPENED, "t1"),
            pair(OPENED, "t2"),
            pair(CLOSED, "t1"),
            pair(OPENED, "t3"),
            pair(CLOSED, "t2"),
        ]
    );
    let positions = positions(&accepted);
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{positions:?}"
    );
}

/// Events committed and never accepted before a SIGKILL are posted after the restart, in commit
/// order, before the events the restarted run commits.
#[test]
fn events_pending_at_a_restart_are_posted_in_commit_order() {
    let stub = Stub::start(|_, _| Reply::Status(503));
    let directory = scratch();
    let state = directory.path().join("state");
    let environment = environment(
        directory.path(),
        &stub.url(),
        &[OPENED, CLOSED],
        "POST",
        2000,
    );
    let served = tasks(&environment, &state);
    open(&served, "t1");
    open(&served, "t2");
    close(&served, "t1");
    stub.wait("a refused post", Duration::from_secs(10), |got| {
        !got.is_empty()
    });
    served.kill();

    stub.answer(|_, _| Reply::Status(200));
    let again = tasks(&environment, &state);
    open(&again, "t3");
    let accepted = stub.wait_accepted(4, Duration::from_secs(10));
    assert_eq!(
        named(&accepted),
        [
            pair(OPENED, "t1"),
            pair(OPENED, "t2"),
            pair(CLOSED, "t1"),
            pair(OPENED, "t3"),
        ]
    );
}

/// `board`'s events and `ledger`'s go to one call on two lanes: a webhook that refuses every
/// body of `board` holds back none of `ledger`'s, which arrive in `ledger`'s commit order.
#[test]
fn each_component_is_posted_on_a_lane_of_its_own() {
    let stub = Stub::start(|_, body| {
        if body["component"] == "board" {
            Reply::Status(503)
        } else {
            Reply::Status(200)
        }
    });
    let directory = scratch();
    let environment = environment(
        directory.path(),
        &stub.url(),
        &[CLOSED, RECORDED],
        "POST",
        2000,
    );
    let served = tasks(&environment, &directory.path().join("state"));
    open(&served, "t1");
    close(&served, "t1");
    open(&served, "t2");
    close(&served, "t2");

    let accepted = stub.wait_accepted(2, Duration::from_secs(10));
    assert_eq!(
        named(&accepted),
        [pair(RECORDED, "t1"), pair(RECORDED, "t2")]
    );
    assert!(
        accepted.iter().all(|body| body["component"] == "ledger"),
        "{accepted:#?}"
    );
    let positions = positions(&accepted);
    assert!(positions[0] < positions[1], "{positions:?}");
    let refused: Vec<Got> = stub
        .got()
        .into_iter()
        .filter(|got| got.status == Some(503))
        .collect();
    assert!(
        !refused.is_empty()
            && refused
                .iter()
                .all(|got| got.body["component"] == "board" && got.body["event"] == CLOSED),
        "board's TaskClosed is posted and refused meanwhile: {refused:#?}"
    );
}

/// The acceptance: "several events of one component arrive with strictly rising `position`",
/// and the Outcome: `position` is the event's store position. A write that publishes two events
/// posts both with one position, the write's, so a consumer that deduplicates the at-least-once
/// posts by component and position drops the second.
#[test]
fn the_events_of_one_write_are_posted_with_strictly_rising_positions() {
    let stub = Stub::start(|_, _| Reply::Status(200));
    let directory = scratch();
    let environment = environment(
        directory.path(),
        &stub.url(),
        &[NOTE_OPENED, NOTE_COUNTED],
        "POST",
        2000,
    );
    let served = listening(run(
        NOTES,
        NOTES_HTTP,
        &environment,
        &directory.path().join("state"),
        &[],
    ));
    for id in ["n1", "n2"] {
        let (status, answer) = exchange(
            served.address,
            "PUT",
            &format!("/notes/{id}"),
            Some(&json!({"title": format!("note {id}")})),
        );
        assert_eq!(status, 201, "{answer}");
    }
    let accepted = stub.wait_accepted(4, Duration::from_secs(10));
    let positions = positions(&accepted);
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "positions rise strictly: {positions:?} for {:?}",
        named(&accepted)
    );
    assert_eq!(
        named(&accepted),
        [
            pair(NOTE_OPENED, "n1"),
            pair(NOTE_COUNTED, "n1"),
            pair(NOTE_OPENED, "n2"),
            pair(NOTE_COUNTED, "n2"),
        ]
    );
}

/// `--drain 0`: the run takes no request after SIGTERM, and the egress stops "each after its post
/// under way". A lane with a backlog against a webhook that answers `200` after 700 ms goes on
/// posting the rest of its page after the stop, so the run exits only once the backlog is posted.
#[test]
fn a_run_stops_its_egress_after_the_post_under_way() {
    let stub = Stub::start(|_, _| Reply::Status(503));
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 5000);
    let mut served = listening(run(
        SPEC,
        BINDING,
        &environment,
        &directory.path().join("state"),
        &["--drain", "0"],
    ));
    for index in 0..15 {
        open(&served, &format!("t{index}"));
    }
    stub.wait("a refused post", Duration::from_secs(10), |got| {
        !got.is_empty()
    });
    stub.answer(|_, _| Reply::After(Duration::from_millis(700), 200));
    stub.wait("a post answered 200", Duration::from_secs(10), |got| {
        got.iter().any(is_2xx)
    });

    let signalled = Instant::now();
    rustix::process::kill_process(
        rustix::process::Pid::from_child(&served.child),
        rustix::process::Signal::TERM,
    )
    .expect("SIGTERM is sent");
    let status = loop {
        if let Some(status) = served.child.try_wait().expect("the run is waited on") {
            break status;
        }
        assert!(
            signalled.elapsed() < Duration::from_secs(30),
            "run did not exit within 30 s of SIGTERM"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let took = signalled.elapsed();
    let posted_after = stub.got().iter().filter(|got| got.at > signalled).count();
    assert_eq!(status.code(), Some(143));
    assert!(
        took < Duration::from_secs(4),
        "the run exited {took:?} after SIGTERM with --drain 0, and was sent {posted_after} posts \
         after the signal"
    );
}

/// An `[egress]` entry naming an event no component publishes: exit 3 before listening, naming
/// the event.
#[test]
fn an_egress_naming_an_unpublished_event_exits_3_naming_it() {
    let stub = Stub::start(|_, _| Reply::Status(200));
    let directory = scratch();
    let environment = environment(
        directory.path(),
        &stub.url(),
        &["tasks.board.TaskLost"],
        "POST",
        2000,
    );
    let output = run(
        SPEC,
        BINDING,
        &environment,
        &directory.path().join("state"),
        &[],
    )
    .current_dir(root())
    .stdin(Stdio::null())
    .output()
    .expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(
        stderr.contains("[egress] posts tasks.board.TaskLost, but no component of the specification publishes an event of that name"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("listening"));
}

/// An `[egress]` call whose method is not `POST` is refused, naming why.
#[test]
fn an_egress_to_a_put_call_is_refused_naming_post() {
    let stub = Stub::start(|_, _| Reply::Status(200));
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "PUT", 2000);
    let output = run(
        SPEC,
        BINDING,
        &environment,
        &directory.path().join("state"),
        &[],
    )
    .current_dir(root())
    .stdin(Stdio::null())
    .output()
    .expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{output:?}");
    assert!(
        stderr.contains(
            "[http.hook] is named by [egress], which posts each event: its method must be POST"
        ),
        "{stderr}"
    );
    assert!(stub.got().is_empty());
}

/// A run reading stdin posts nothing: an `[egress]` makes it exit 1, naming the egress.
#[test]
fn a_stdin_run_with_an_egress_exits_1() {
    let stub = Stub::start(|_, _| Reply::Status(200));
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 2000);
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment"])
        .arg(&environment)
        .arg("--state")
        .arg(directory.path().join("state"))
        .arg("--stdin")
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr.contains("would never post an event"), "{stderr}");
    assert!(stub.got().is_empty());
}

/// A `307` is no `2xx`: it is not followed, and the event is posted again to the call's URL.
#[test]
fn a_redirect_is_not_followed_and_the_event_is_posted_again() {
    let stub = Stub::start(|index, _| {
        if index < 2 {
            Reply::Redirect("/elsewhere")
        } else {
            Reply::Status(200)
        }
    });
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 2000);
    let served = tasks(&environment, &directory.path().join("state"));
    open(&served, "t1");
    stub.wait_accepted(1, Duration::from_secs(10));
    std::thread::sleep(Duration::from_millis(500));
    let got = stub.got();
    assert_eq!(got.len(), 3, "{got:#?}");
    assert!(
        got.iter().all(|got| got.line.starts_with("POST /events ")),
        "{got:#?}"
    );
    assert_eq!(named(&stub.accepted()), [pair(OPENED, "t1")]);
}

/// A connection closed without an answer and a webhook silent past the call's timeout are both
/// posted again; a `200` with a short body ends it.
#[test]
fn a_closed_connection_and_a_timeout_are_posted_again_until_a_2xx() {
    let stub = Stub::start(|index, _| match index {
        0 => Reply::Closed,
        1 => Reply::Silent(Duration::from_secs(2)),
        _ => Reply::WithBody(200, 16),
    });
    let directory = scratch();
    let environment = environment(directory.path(), &stub.url(), &[OPENED], "POST", 500);
    let served = tasks(&environment, &directory.path().join("state"));
    open(&served, "t1");
    stub.wait_accepted(1, Duration::from_secs(10));
    std::thread::sleep(Duration::from_millis(700));
    let got = stub.got();
    assert_eq!(got.len(), 3, "{got:#?}");
    assert!(
        got.iter().all(|each| each.body == got[0].body),
        "each attempt carries one body: {got:#?}"
    );
    assert_eq!(named(&stub.accepted()), [pair(OPENED, "t1")]);
}
