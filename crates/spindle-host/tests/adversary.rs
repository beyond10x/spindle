//! Adversary cases for the host: what it promises against what it does.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, RETRY_ATTEMPTS};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/adversary")
        .join(name)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-memory\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn host(spec: &Path, environment: &Environment, state: &Path) -> Result<Host, HostError> {
    let loaded = spindle_spec::load(spec).expect("the specification compiles");
    Host::new(loaded.ir, environment, Arc::new(ManualClock::new()), state)
}

/// `pin-post` says `on_failure: retry`. Each attempt is refused while the board does not exist.
/// ESS retries at once and never defers: ESS 0.86.0's interpreter, given this fixture, `Post`
/// and then `OpenBoard`, retries `Pin` inline until its attempt budget is spent ("unbounded retry
/// exceeded the finite execution budget") and never pins after `OpenBoard`. So the host must
/// either refuse the binding as unsupported, or retry it at once until its limit and give it up.
/// Running it once and dropping it is what `on_failure: drop` says, not `retry`.
#[test]
fn a_binding_that_says_retry_is_retried_until_exhausted() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = match host(&fixture("notice"), &laptop(), state.path()) {
        Err(HostError::Unsupported(_) | HostError::Route(_)) => return,
        Err(other) => panic!("neither admitted nor refused as unsupported: {other}"),
        Ok(host) => host,
    };
    let posted = host
        .submit(
            "notice.source.Post",
            object(json!({"board_id": "b1", "beat": "first"})),
        )
        .expect("the post runs");
    let pinned = posted
        .refused
        .iter()
        .find(|refused| refused.command == "notice.sink.Pin")
        .unwrap_or_else(|| panic!("the refused Pin is reported: {posted:?}"));
    const { assert!(RETRY_ATTEMPTS > 1) };
    assert_eq!(
        pinned.attempts, RETRY_ATTEMPTS,
        "pin-post says on_failure: retry, so its refused Pin is attempted to the limit: {posted:?}"
    );
    assert_eq!(
        host.counters("notice-sink")
            .expect("the sink runs")
            .value("binding.pin-post.exhausted"),
        Some(1)
    );
    host.submit("notice.sink.OpenBoard", object(json!({"board_id": "b1"})))
        .expect("the board opens");
    let board = host
        .view("notice.sink.BoardState", &json!("b1"))
        .expect("the view answers")
        .expect("the board exists");
    assert_ne!(
        board.get("last"),
        Some(&json!("first")),
        "the given-up envelope is not redelivered once the board exists"
    );
}

/// Runs only when spawned by the case below: `echo` binds each side's event to the other side's
/// command, so one submit has no end unless the host bounds it.
#[test]
#[ignore = "spawned by bindings_that_trigger_each_other_do_not_hang_the_host"]
fn echo_child_submits_once() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let outcome = host(&fixture("echo"), &laptop(), state.path())
        .and_then(|mut host| host.submit("echo.left.Send", object(json!({"n": 1}))));
    println!("ECHO-CHILD-RETURNED {}", outcome.is_ok());
}

#[test]
fn bindings_that_trigger_each_other_do_not_hang_the_host() {
    let mut child = Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "echo_child_submits_once",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the child starts");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("the child is observable") {
            assert!(status.success(), "the child failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "Host::new admitted echo (left-to-right and right-to-left trigger each other) \
                 and one submit of echo.left.Send had not returned after 5 s: settle has no bound \
                 and no refusal of a binding cycle"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// ESS (`omitted-or-null-presence.md` § 2): a view field projecting an entity field that declares
/// `presence: null_when_absent` carries that policy, and a row whose value is absent answers the
/// member as `null`; leaving it out is refused under that policy.
#[test]
fn a_view_field_that_carries_null_when_absent_answers_null() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(&fixture("memo"), &laptop(), state.path()).expect("laptop admits memo");
    host.submit(
        "memo.notes.Write",
        object(json!({"note_id": "n1", "title": "first"})),
    )
    .expect("the note is written");
    let answer = host
        .view("memo.notes.NoteState", &json!("n1"))
        .expect("the view answers")
        .expect("the note exists");
    assert_eq!(
        Value::Object(answer),
        json!({"note_id": "n1", "title": "first", "remark": null}),
        "remark carries presence: null_when_absent, so an absent remark is answered as null"
    );
}

/// `b10x-spindle play --target-score <n>` passes any integer to `OpenMatch`. With 0 the match
/// opens, `west_needs`/`east_needs` start at 0 and every point takes them below zero, and no
/// `*-wins` guard (`*_needs == 1`) can ever hold again: the match is never won and the frame shows
/// negative points still needed.
#[test]
fn a_match_opened_with_target_score_zero_is_refused_or_ends_without_negative_needs() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    let clock = Arc::new(ManualClock::new());
    let mut host = Host::new(loaded.ir, &in_memory(), clock.clone(), state.path())
        .expect("the in-memory environment admits rally");
    if host
        .submit(
            "rally.table.OpenMatch",
            object(json!({"match_id": "m0", "target_score": 0})),
        )
        .is_err()
    {
        return;
    }
    host.start("tick", object(json!({"match_id": "m0"})))
        .expect("the tick binding starts");
    for tick in 0..4000 {
        clock.advance(Duration::from_secs(1));
        host.advance().expect("the host advances");
        let view = host
            .view("rally.table.MatchState", &json!("m0"))
            .expect("the view answers")
            .expect("the match exists");
        for side in ["west_needs", "east_needs"] {
            let needs = view[side].as_i64().expect("an integer");
            assert!(
                needs >= 0,
                "after {tick} ticks {side} is {needs}: a match opened with target_score 0 \
                 counts below zero and can never be won; {}",
                spindle_host::rally::frame(&view).unwrap_or_default()
            );
        }
        if view.get("winner").is_some() {
            return;
        }
    }
    panic!("a match opened with target_score 0 had no winner after 4000 ticks");
}

/// `Host::new` documents `HostError::Unsupported` "when admission bound something this host does
/// not run". Spindle's own specification has `external:` outcomes; laptop binds `external.*` to
/// `operator-prompt`, and the host executes every command with `NoExternals`, which answers none.
#[test]
fn a_host_that_answers_no_externals_refuses_a_specification_that_has_them() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    match host(&root().join("ess"), &laptop(), state.path()) {
        Err(HostError::Unsupported(_)) => {}
        Err(other) => panic!("refused, but not as unsupported: {other}"),
        Ok(host) => panic!(
            "admitted with external outcomes bound to operator-prompt, which this host does not \
             run: {:?}",
            host.placements()
        ),
    }
}
