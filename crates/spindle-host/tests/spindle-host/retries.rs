//! An event binding with `on_failure: retry`: retried by default, or, when the host is told to drop
//! its failed invocations, run as a `drop` binding, each failed invocation run once.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, HostError, ManualClock, Retries};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `pad`, which a request reaches, and `index`, which the binding `index-note` (`on_failure:
/// retry`) reaches with a command the kernel does not run.
fn fixture() -> PathBuf {
    root().join("crates/spindle-cli/tests/fixtures/retried")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn host(retries: Retries) -> (Result<Host, HostError>, tempfile::TempDir) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&fixture()).expect("the fixture compiles");
    let host = Host::with_retries(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
        retries,
    );
    (host, state)
}

#[test]
fn the_retry_bindings_are_named() {
    let loaded = spindle_spec::load(&fixture()).expect("the fixture compiles");
    assert_eq!(spindle_host::retried(&loaded.ir), ["index-note"]);
    let ingress = spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/ingress"))
        .expect("the fixture compiles");
    assert!(spindle_host::retried(&ingress.ir).is_empty());
}

#[test]
fn a_retry_binding_is_retried_by_default() {
    let (host, _state) = host(Retries::default());
    let mut host = host.expect("the host starts with a retry binding");
    let loaded = spindle_spec::load(&fixture()).expect("the fixture compiles");
    assert_eq!(
        spindle_host::uncarried(&loaded.ir),
        None,
        "only escalate is not carried"
    );
    let report = host
        .submit(
            "notes.pad.WriteNote",
            object(json!({"note_id": "n1", "text": "kept"})),
        )
        .expect("the command runs");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(
        report.failed[0].attempts,
        spindle_host::RETRY_ATTEMPTS,
        "the invocation the kernel does not run is retried to the limit, then given up"
    );
}

#[test]
fn a_dropped_retry_runs_its_failed_invocation_once() {
    let (host, _state) = host(Retries::Dropped);
    let mut host = host.expect("the host starts when retries are dropped");
    let report = host
        .submit(
            "notes.pad.WriteNote",
            object(json!({"note_id": "n1", "text": "kept"})),
        )
        .expect("the command runs");
    assert_eq!(report.ran.len(), 1, "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    let failed = &report.failed[0];
    assert_eq!(failed.command, "notes.index.IndexNote");
    assert_eq!(
        failed.cause,
        Cause::Delivered {
            binding: "index-note".to_owned()
        }
    );
    let later = host.advance().expect("the host goes on");
    assert!(
        later.failed.is_empty() && later.ran.is_empty(),
        "the failed invocation is not run again: {later:?}"
    );
    assert_eq!(
        host.published()
            .expect("the events read")
            .iter()
            .map(|published| published.event.name.as_str())
            .collect::<Vec<_>>(),
        ["notes.pad.NoteWritten"]
    );
}
