//! The native build keeps everything the wasm build leaves out: the default features still open a
//! store bound to `sqlite` in its file, and the same command the wasm test runs on memory runs on
//! it.

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

#[test]
fn the_native_build_is_unchanged() {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    let laptop = Environment::parse(&text).expect("laptop.toml is an environment");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded =
        spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notebooks"))
            .expect("the notebooks fixture compiles");
    let mut host = Host::new(
        loaded.ir,
        &laptop,
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the default build admits the notebooks fixture on sqlite");

    host.submit(
        "desk.notebooks.OpenNotebook",
        object(json!({"title": "Field notes"})),
    )
    .expect("OpenNotebook runs");
    let identity =
        host.published().expect("the events read")[0].event.payload["notebook_id"].clone();
    let card = host
        .view("desk.notebooks.NotebookCard", &identity)
        .expect("the view reads")
        .expect("the opened notebook has a card");
    assert_eq!(card["title"], json!("Field notes"));
    assert!(
        state.path().join("desk.sqlite").is_file(),
        "the desk's state rests in its SQLite file"
    );
}
