//! A list an outcome appends to, `sets: {f: {append: input.<path>}}` (ess/31), is read back whole
//! by a view: the kernel's `list_append` fixture, run by a host on SQLite and on memory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

const NOTEBOOK_ID: &str = "4d1c7a2e-9b3f-4c5d-8e6f-0a1b2c3d4e5f";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-kernel/tests/fixtures/list_append")
}

/// The shipped laptop environment, which binds `store.durable` to `sqlite`.
fn laptop() -> Environment {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

/// An environment that binds every store to `memory`.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"list-append-memory\"\n",
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
    ))
    .expect("the in-memory environment parses")
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

#[test]
fn a_view_lists_the_appended_elements() {
    for (name, environment) in [("sqlite", laptop()), ("memory", in_memory())] {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let loaded = spindle_spec::load(&fixture()).expect("the list_append fixture compiles");
        let mut host = Host::new(
            loaded.ir,
            &environment,
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("{name} admits the list_append fixture: {error}"));
        let submit = |host: &mut Host, command: &str, sent: Value| {
            host.submit(command, input(sent))
                .unwrap_or_else(|error| panic!("{name}: {command} runs: {error}"));
        };
        submit(
            &mut host,
            "scribe.notebook.OpenNotebook",
            json!({"notebook_id": NOTEBOOK_ID, "title": "field notes"}),
        );
        for page in ["dawn chorus", "low tide"] {
            submit(
                &mut host,
                "scribe.notebook.AddPage",
                json!({"notebook_id": NOTEBOOK_ID, "page": page}),
            );
        }
        submit(
            &mut host,
            "scribe.notebook.AddMargin",
            json!({"notebook_id": NOTEBOOK_ID, "note": "see p. 2"}),
        );
        let card = host
            .view("scribe.notebook.NotebookCard", &json!(NOTEBOOK_ID))
            .unwrap_or_else(|error| panic!("{name}: the view answers: {error}"))
            .unwrap_or_else(|| panic!("{name}: the view holds the notebook"));
        assert_eq!(
            Value::Object(card),
            json!({
                "notebook_id": NOTEBOOK_ID,
                "title": "field notes",
                "pages": ["dawn chorus", "low tide"],
                "margins": ["see p. 2"],
            }),
            "{name}: the view row reads each list whole, in the order it was appended"
        );
    }
}
