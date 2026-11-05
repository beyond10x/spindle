//! Adversary cases for `Host::reset`: a reset that fails part way must not leave a component bound
//! to SQLite answering from a store in memory.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn host(state: &Path) -> Host {
    let loaded = spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/ingress"))
        .expect("the fixture compiles");
    Host::new(loaded.ir, &laptop(), Arc::new(ManualClock::new()), state).expect("the host starts")
}

fn open(host: &mut Host, task: &str) {
    host.submit(
        "tasks.board.OpenTask",
        object(json!({"task_id": task, "title": task})),
    )
    .expect("the task opens");
}

fn tasks(host: &mut Host) -> Vec<String> {
    host.list("tasks.board.AllTasks")
        .expect("lists")
        .into_iter()
        .map(|row| row["task_id"].as_str().expect("an id").to_owned())
        .collect()
}

/// Restores a directory's mode when dropped, so the temporary directory can be removed.
struct Writable<'a>(&'a Path);

impl Drop for Writable<'_> {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(self.0, std::fs::Permissions::from_mode(0o700));
    }
}

#[test]
fn adversary_a_failed_reset_leaves_no_durable_component_on_memory() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let state = dir.path().join("state");
    std::fs::create_dir(&state).expect("the state directory");
    let mut running = host(&state);
    open(&mut running, "t1");

    let guard = Writable(&state);
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500))
        .expect("the state directory is made read-only");
    let reset = running.reset();
    drop(guard);
    assert!(
        reset.is_err(),
        "a file in a read-only directory cannot be removed"
    );

    let held = tasks(&mut running);
    open(&mut running, "t2");
    let after = tasks(&mut running);
    drop(running);

    let mut restarted = host(&state);
    let durable = tasks(&mut restarted);
    assert_eq!(
        (held, after),
        (
            durable
                .iter()
                .filter(|task| *task != "t2")
                .cloned()
                .collect(),
            durable.clone()
        ),
        "after a failed reset the running host answers what its SQLite file holds, and a task \
         opened afterwards is durable: restarted host holds {durable:?}"
    );
}
