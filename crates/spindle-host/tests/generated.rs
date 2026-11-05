//! A field an outcome marks `{generated: true}` is minted by the kernel, and a view reads the
//! stored value back: the kernel's `generated` fixture, run by a host on SQLite.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ess_primitives::Rfc3339Instant;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

const BATCH_ID: &str = "9b2f4c1e-7a3d-4e5f-8a6b-1c2d3e4f5a6b";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-kernel/tests/fixtures/generated")
}

/// The shipped laptop environment, which binds `store.durable` to `sqlite`.
fn laptop() -> Environment {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

#[test]
fn a_generated_timestamp_is_the_bound_clocks_time() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&fixture()).expect("the generated fixture compiles");
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_893_553_445);
    let clock = Arc::new(ManualClock::at(start));
    let mut host =
        Host::new(loaded.ir, &laptop(), clock.clone(), state.path()).expect("the host starts");
    let fired_at = |host: &mut Host, batch: &str| {
        host.submit(
            "forge.batch.StartBatch",
            input(json!({"batch_id": batch, "label": "kiln-7"})),
        )
        .expect("the batch starts");
        let card = host
            .view("forge.batch.BatchCard", &json!(batch))
            .expect("the view reads")
            .expect("the view holds the batch");
        let stamp = card["fired_at"].as_str().expect("a stamp").to_owned();
        Rfc3339Instant::parse_rfc3339(&stamp)
            .unwrap_or_else(|| panic!("{stamp} is an RFC 3339 instant"))
    };
    let expected = |text: &str| Rfc3339Instant::parse_rfc3339(text).expect("an instant");

    assert_eq!(
        fired_at(&mut host, BATCH_ID),
        expected("2030-01-02T03:04:05Z"),
        "the stamp is the clock's time, not the machine's"
    );
    assert_eq!(
        fired_at(&mut host, "1c2d3e4f-5a6b-4c7d-8e9f-0a1b2c3d4e5f"),
        expected("2030-01-02T03:04:05Z"),
        "a clock that has not moved stamps the same instant"
    );
    clock.advance(Duration::from_secs(3600));
    assert_eq!(
        fired_at(&mut host, "2d3e4f5a-6b7c-4d8e-9f0a-1b2c3d4e5f6a"),
        expected("2030-01-02T04:04:05Z"),
        "the stamp moves with the clock"
    );
}

#[test]
fn a_generated_stored_field_is_minted_and_kept() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&fixture()).expect("the generated fixture compiles");
    let mut host = Host::new(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the host starts");

    let report = host
        .submit(
            "forge.batch.StartBatch",
            input(json!({"batch_id": BATCH_ID, "label": "kiln-7"})),
        )
        .expect("the batch starts");
    let published = report.ran[0].executed.events[0].payload["sequence"].clone();
    assert!(published.is_i64(), "a minted Integer: {published}");

    let card = host
        .view("forge.batch.BatchCard", &json!(BATCH_ID))
        .expect("the view reads")
        .expect("the view holds the batch");
    assert_eq!(
        card["sequence"], published,
        "the view reads the published sequence"
    );
    for field in ["serial", "token", "note", "fired_at"] {
        assert!(
            card[field].as_str().is_some_and(|text| !text.is_empty()),
            "the view reads the minted `{field}`: {card:?}"
        );
    }
    assert_eq!(card["label"], json!("kiln-7"));

    host.submit(
        "forge.batch.RelabelBatch",
        input(json!({"batch_id": BATCH_ID, "label": "kiln-8"})),
    )
    .expect("the batch relabels");
    let kept = host
        .view("forge.batch.BatchCard", &json!(BATCH_ID))
        .expect("the view reads")
        .expect("the view holds the batch");
    assert_eq!(kept["label"], json!("kiln-8"));
    for field in ["serial", "sequence", "fired_at", "token", "note"] {
        assert_eq!(kept[field], card[field], "`{field}` keeps its minted value");
    }
}
