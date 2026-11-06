//! Adversary cases for view filters over the lifecycle state: every filter here is one
//! `ess specify validate` (0.86.0) admits, and each is answered with exactly the rows ESS's own
//! evaluation holds, or refused naming the filter; never a subset or a superset.
//!
//! Three parcels: `p1` packed and labelled `Packed`, `p2` shipped and labelled `Plain`, `p3`
//! delivered and labelled `Fragile`. The first label spells a state on purpose, so a filter that
//! compares the state with the field `label` holds for `p1` alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/states")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

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

/// The states fixture with one more view, `parcels.depot.Probe`, whose filter is `filter`
/// (YAML flow), projecting the identity and the label; the three parcels submitted.
fn host_with(filter: &str, check: impl FnOnce(&mut Host)) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let spec = dir.path().join("spec");
    std::fs::create_dir(&spec).expect("spec directory");
    for name in ["system.yaml", "components.yaml"] {
        std::fs::copy(fixture().join(name), spec.join(name)).expect("copy fixture");
    }
    let mut depot =
        std::fs::read_to_string(fixture().join("depot.yaml")).expect("depot.yaml reads");
    depot.push_str(&format!(
        concat!(
            "  - name: parcels.depot.Probe\n",
            "    source: parcels.depot.Parcel\n",
            "    consistency: read_your_writes\n",
            "    filter: {}\n",
            "    fields:\n",
            "      - name: parcel_id\n",
            "        type: parcels.depot.ParcelId\n",
            "      - name: label\n",
            "        type: String\n",
        ),
        filter
    ));
    std::fs::write(spec.join("depot.yaml"), depot).expect("depot.yaml writes");
    let loaded = spindle_spec::load(&spec).unwrap_or_else(|error| {
        panic!("`{filter}` compiles, as ess specify validate admits it: {error}")
    });
    let state = dir.path().join("state");
    std::fs::create_dir(&state).expect("state directory");
    let mut host = Host::new(
        loaded.ir,
        &in_memory(),
        Arc::new(ManualClock::new()),
        &state,
    )
    .expect("the host admits the probe");
    for (parcel, label) in [("p1", "Packed"), ("p2", "Plain"), ("p3", "Fragile")] {
        submit(&mut host, "parcels.depot.PackParcel", parcel, Some(label));
    }
    submit(&mut host, "parcels.depot.ShipParcel", "p2", None);
    submit(&mut host, "parcels.depot.ShipParcel", "p3", None);
    submit(&mut host, "parcels.depot.DeliverParcel", "p3", None);
    check(&mut host);
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

fn ids(rows: Vec<Map<String, Value>>) -> Vec<String> {
    rows.into_iter()
        .map(|row| row["parcel_id"].as_str().expect("an id").to_owned())
        .collect()
}

/// The probe answered with exactly `expected`, by list and by one-row read.
fn answered(filter: &str, expected: &[&str]) {
    host_with(filter, |host| {
        let rows = host
            .list("parcels.depot.Probe")
            .unwrap_or_else(|error| panic!("`{filter}` reads only the state: {error}"));
        assert_eq!(ids(rows), expected, "`{filter}`: the rows ESS holds");
        for parcel in ["p1", "p2", "p3"] {
            let row = host
                .view("parcels.depot.Probe", &json!(parcel))
                .unwrap_or_else(|error| panic!("`{filter}` one-row read: {error}"));
            assert_eq!(
                row.is_some(),
                expected.contains(&parcel),
                "`{filter}`: one-row read of {parcel}"
            );
        }
    });
}

/// The probe either refused naming its filter (the text contains `named`), or answered with
/// exactly `expected`, by list and by one-row read.
fn answered_or_refused(filter: &str, named: &str, expected: &[&str]) {
    host_with(filter, |host| match host.list("parcels.depot.Probe") {
        Err(HostError::Unsupported(text)) => {
            assert!(
                text.contains("parcels.depot.Probe") && text.contains(named),
                "`{filter}`: the refusal names the view and `{named}`: {text}"
            );
            match host.view("parcels.depot.Probe", &json!("p1")) {
                Err(HostError::Unsupported(one)) => {
                    assert!(one.contains(named), "`{filter}`: one-row refusal: {one}")
                }
                other => panic!("`{filter}`: list refused, one-row read not: {other:?}"),
            }
        }
        Ok(rows) => {
            assert_eq!(
                ids(rows),
                expected,
                "`{filter}` answered: exactly the rows ESS holds, not those the state alone \
                     decides"
            );
            for parcel in ["p1", "p2", "p3"] {
                let row = host
                    .view("parcels.depot.Probe", &json!(parcel))
                    .unwrap_or_else(|error| panic!("`{filter}` one-row read: {error}"));
                assert_eq!(
                    row.is_some(),
                    expected.contains(&parcel),
                    "`{filter}`: one-row read of {parcel}"
                );
            }
        }
        Err(other) => panic!("`{filter}`: neither answered nor refused by name: {other}"),
    });
}

#[test]
fn none_of_over_states_is_answered() {
    answered("{state: {none_of: [Delivered]}}", &["p1", "p2"]);
}

#[test]
fn not_in_over_states_is_answered() {
    answered("{state: {not_in: [Delivered, Packed]}}", &["p2"]);
}

#[test]
fn an_empty_in_list_holds_for_no_row() {
    answered("{state: {in: []}}", &[]);
}

#[test]
fn an_empty_none_of_list_holds_for_every_row() {
    answered("{state: {none_of: []}}", &["p1", "p2", "p3"]);
}

#[test]
fn a_quoted_state_literal_is_answered() {
    answered("state == \"Packed\"", &["p1"]);
}

#[test]
fn the_mapping_forms_eq_and_ne_are_answered() {
    answered("{state: {eq: Packed}}", &["p1"]);
    answered("{state: {ne: Packed}}", &["p2", "p3"]);
}

#[test]
fn not_over_a_field_inside_any_is_refused_or_answered_whole() {
    answered_or_refused(
        "{any: [state == Packed, {not: label == Plain}]}",
        "label",
        &["p1", "p3"],
    );
}

#[test]
fn in_over_a_field_inside_any_is_refused_or_answered_whole() {
    answered_or_refused(
        "{any: [state == Shipped, {label: {in: [Fragile]}}]}",
        "label",
        &["p2", "p3"],
    );
}

#[test]
fn a_double_not_over_a_mixed_any_is_refused_or_answered_whole() {
    answered_or_refused(
        "{not: {not: {any: [state == Packed, label == Fragile]}}}",
        "label",
        &["p1", "p3"],
    );
}

#[test]
fn state_compared_with_a_bare_field_word_is_refused_or_answered_as_esss_field_read() {
    answered_or_refused("state == label", "label", &["p1"]);
}

#[test]
fn state_compared_with_an_explicit_fact_operand_is_refused_or_answered() {
    answered_or_refused("{state: {eq: {fact: label}}}", "label", &["p1"]);
}

#[test]
fn a_filter_that_always_holds_is_refused_by_name_or_answers_every_row() {
    answered_or_refused("{all: []}", "filter", &["p1", "p2", "p3"]);
}
