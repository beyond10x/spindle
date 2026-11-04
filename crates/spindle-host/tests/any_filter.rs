//! View filters over several lifecycle states: `any:` (and `all:`, `not:`, `in:`) of comparisons
//! of the state alone are answered from the store, on memory and SQLite; a filter that also reads
//! a stored field is refused by name, never answered with the rows the state alone decides.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/states")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// The shipped laptop environment, which binds `store.durable` to `sqlite`.
fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

/// An environment that binds every store to `memory`.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"states-memory\"\n",
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

/// Runs `check` once on a host over SQLite and once on a host over memory, each holding three
/// parcels: `p1` packed, `p2` shipped and `p3` delivered, `p3` labelled `Fragile`.
fn on_every_store(check: impl Fn(&str, &mut Host)) {
    for (name, environment) in [("sqlite", laptop()), ("memory", in_memory())] {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let loaded = spindle_spec::load(&fixture()).expect("the states fixture compiles");
        let mut host = Host::new(
            loaded.ir,
            &environment,
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("{name} admits the states fixture: {error}"));
        for (parcel, label) in [("p1", "Plain"), ("p2", "Plain"), ("p3", "Fragile")] {
            submit(&mut host, "parcels.depot.PackParcel", parcel, Some(label));
        }
        submit(&mut host, "parcels.depot.ShipParcel", "p2", None);
        submit(&mut host, "parcels.depot.ShipParcel", "p3", None);
        submit(&mut host, "parcels.depot.DeliverParcel", "p3", None);
        check(name, &mut host);
    }
}

fn submit(host: &mut Host, command: &str, parcel: &str, label: Option<&str>) {
    let mut input = object(json!({"parcel_id": parcel}));
    if let Some(label) = label {
        input.insert("label".to_owned(), json!(label));
    }
    let report = host
        .submit(command, input)
        .unwrap_or_else(|error| panic!("{command} {parcel}: {error}"));
    assert!(
        report.refused.is_empty() && report.failed.is_empty(),
        "{command} {parcel} runs: {report:?}"
    );
}

#[test]
fn a_view_over_two_states_lists_both() {
    on_every_store(|store, host| {
        let rows = host
            .list("parcels.depot.Undelivered")
            .expect("`any: [state == Packed, state == Shipped]` is answered");
        assert_eq!(
            rows.into_iter().map(Value::Object).collect::<Vec<_>>(),
            vec![
                json!({"parcel_id": "p1", "state": "Packed"}),
                json!({"parcel_id": "p2", "state": "Shipped"}),
            ],
            "{store}: the packed and the shipped parcel are rows, the delivered one is not"
        );
        assert_eq!(
            host.view("parcels.depot.Undelivered", &json!("p2"))
                .expect("a one-row read is answered"),
            Some(object(json!({"parcel_id": "p2", "state": "Shipped"}))),
            "{store}: a one-row read keeps a parcel in the second state"
        );
        assert_eq!(
            host.view("parcels.depot.Undelivered", &json!("p3"))
                .expect("a one-row read is answered"),
            None,
            "{store}: a one-row read drops the parcel in the third state"
        );
    });
}

#[test]
fn an_any_filter_mixing_state_and_field_comparisons_is_answered_or_refused_by_name() {
    on_every_store(|store, host| {
        match host.list("parcels.depot.UndeliveredOrFragile") {
            Err(HostError::Unsupported(text)) => assert!(
                text.contains("parcels.depot.UndeliveredOrFragile")
                    && text.contains("label == Fragile"),
                "{store}: the refusal names the view and its filter: {text}"
            ),
            Ok(rows) => assert_eq!(
                rows.into_iter().map(Value::Object).collect::<Vec<_>>(),
                vec![
                    json!({"parcel_id": "p1", "label": "Plain"}),
                    json!({"parcel_id": "p3", "label": "Fragile"}),
                ],
                "{store}: an answered mixed filter holds the packed parcel and the fragile \
                 delivered one, never only what the state decides"
            ),
            Err(other) => {
                panic!("{store}: the mixed filter is neither answered nor refused by name: {other}")
            }
        }
        match host.view("parcels.depot.UndeliveredOrFragile", &json!("p3")) {
            Err(HostError::Unsupported(text)) => assert!(
                text.contains("label == Fragile"),
                "{store}: a one-row read is refused the same way: {text}"
            ),
            Ok(row) => assert_eq!(
                row,
                Some(object(json!({"parcel_id": "p3", "label": "Fragile"}))),
                "{store}: an answered one-row read keeps the fragile delivered parcel"
            ),
            Err(other) => {
                panic!("{store}: a one-row read is neither answered nor refused by name: {other}")
            }
        }
    });
}

#[test]
fn all_not_and_in_over_the_state_are_answered() {
    on_every_store(|store, host| {
        let rows = host
            .list("parcels.depot.Moving")
            .expect("`all`, `not` and `in` over the state are answered");
        assert_eq!(
            rows.into_iter().map(Value::Object).collect::<Vec<_>>(),
            vec![json!({"parcel_id": "p2", "state": "Shipped"})],
            "{store}: only the shipped parcel is neither packed nor delivered"
        );
    });
}
