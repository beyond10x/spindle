//! `GET /__spindle/topology` answered by the transport-free router on a host whose timer fires
//! (`story:live-topology`): `run` never starts a periodic binding, so only a host driven on an
//! injected clock shows a periodic binding's `invokes` edge counting the ticks its timer fired.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Request, Surface, route};

/// How many ticks the test lets the timer fire.
const TICKS: u64 = 3;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every store in memory: this crate's host carries no SQLite store.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"topology\"\n",
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
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// `GET path` on `host` at the routes of `surface`: the status and the body as text.
fn get(surface: &Surface, host: &mut Host, path: &str) -> (u16, String) {
    let routed = route(
        surface,
        host,
        Request {
            method: "GET".to_owned(),
            path: path.to_owned(),
            query: String::new(),
            content_type: None,
            body: Vec::new(),
        },
    );
    (routed.response.status, routed.response.body)
}

#[test]
fn a_periodic_bindings_invokes_edge_counts_the_ticks_its_timer_fired() {
    let ir = spindle_spec::load(&root().join("examples/rally/ess"))
        .expect("rally compiles")
        .ir;
    let surface = Surface::new(&ir, &["rally-table".to_owned()], None)
        .expect("the table's surface is routed")
        .observing(16);
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let clock = Arc::new(ManualClock::new());
    let mut host = Host::new(ir, &in_memory(), clock.clone(), state.path())
        .unwrap_or_else(|error| panic!("the host admits rally: {error}"));
    host.submit(
        "rally.table.OpenMatch",
        object(json!({"match_id": "m1", "target_score": 99})),
    )
    .expect("the match opens");
    host.start("tick", object(json!({"match_id": "m1"})))
        .expect("the tick starts");
    for _ in 0..TICKS {
        clock.advance(Duration::from_secs(1));
        host.advance().expect("the host advances");
    }

    let (status, body) = get(&surface, &mut host, "/__spindle/topology");
    assert_eq!(status, 200, "{body}");
    let topology: Value = serde_json::from_str(&body).expect("the topology is JSON");
    let invoking = topology["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["kind"] == "invokes" && edge["from"] == "binding:tick")
        .collect::<Vec<_>>();
    assert_eq!(invoking.len(), 1, "one invokes edge for tick: {topology}");
    assert_eq!(invoking[0]["to"], "command:rally.table.Tick", "{topology}");
    assert!(
        !topology["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .any(|edge| edge["kind"] == "carries" && edge["to"] == "binding:tick"),
        "no event travels a periodic binding: {topology}"
    );

    let (status, metrics) = get(&surface, &mut host, "/__spindle/metrics");
    assert_eq!(status, 200, "{metrics}");
    let fired = format!("spindle_timer_ticks_total{{binding=\"tick\",timed=\"fired\"}} {TICKS}\n");
    assert!(metrics.contains(&fired), "{fired} among {metrics}");
    assert_eq!(
        invoking[0]["traffic"]["count"], TICKS,
        "the ticks the timer fired: {topology}"
    );
}
