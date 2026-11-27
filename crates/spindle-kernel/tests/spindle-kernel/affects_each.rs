//! An `affects:` entry over an input list (`each: {in: input.<list>, as: <name>}`, ess/23, ess/27,
//! ess/35; `story:affects-over-input-list`): for each element, in list order, the entry acts on the
//! row the element names, as ESS's interpreter does (`interpret::execute::set_effects::elements`).
//! With `sets:` it updates that row if held and creates it in the entity's `initial` state if not;
//! a `moves:` or `deletes:` entry acts only on a held row. An element the entry's `where:` does not
//! select is skipped, an element naming the subject is skipped, and `repeated: last_wins` keeps the
//! last element naming a row.
//!
//! The fixture `tests/fixtures/affects-each` holds releases, their records and bins. `Publish`
//! records one row per contract listed, `PublishStable` only those whose version is not the one
//! skipped, `Retire` retires and `Purge` removes the records listed, `Link` links every release
//! listed to the subject, and `Fill` labels every bin listed, by a `Decimal` slot, the last element
//! naming a bin winning.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Executed, MemoryStore, NoExternals, Row, SqliteStore, Store, Stored, execute,
};

use super::related_guards::{Answer, Rows, interpreted, spindled_with};

const RELEASE: &str = "depot.release.Release";
const RECORD: &str = "depot.release.Record";
const BIN: &str = "depot.release.Bin";

const OPEN: &str = "depot.release.OpenRelease";
const ADD_RECORD: &str = "depot.release.AddRecord";
const ADD_BIN: &str = "depot.release.AddBin";
const PUBLISH: &str = "depot.release.Publish";
const PUBLISH_STABLE: &str = "depot.release.PublishStable";
const RETIRE: &str = "depot.release.Retire";
const PURGE: &str = "depot.release.Purge";
const LINK: &str = "depot.release.Link";
const FILL: &str = "depot.release.Fill";

const VIEWS: [(&str, &str, &str); 3] = [
    ("depot.release.ReleaseCard", RELEASE, "release_id"),
    ("depot.release.RecordCard", RECORD, "record_id"),
    ("depot.release.BinCard", BIN, "slot"),
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/affects-each")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the affects-each fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input))
}

fn run(ir: &spindle_spec::EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    send(ir, store, command, input.clone())
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

/// Each store a case runs on, by name: memory, and SQLite in a directory of its own.
fn stores() -> Vec<(&'static str, Box<dyn Store>, Option<tempfile::TempDir>)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let sqlite = SqliteStore::open(&dir.path().join("releases.sqlite")).expect("SQLite opens");
    vec![
        ("memory", Box::new(MemoryStore::default()), None),
        ("sqlite", Box::new(sqlite), Some(dir)),
    ]
}

fn stored(store: &mut dyn Store, entity: &str, identity: &str) -> Option<Stored> {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
}

fn held(store: &mut dyn Store, entity: &str, identity: &str) -> Row {
    stored(store, entity, identity)
        .unwrap_or_else(|| panic!("{entity} {identity} is held"))
        .row
}

/// Every held row of `entity`, by identity, in the order the store scans them.
fn rows(store: &mut dyn Store, entity: &str) -> Vec<(Value, Stored)> {
    store
        .scan(entity)
        .expect("the store scans")
        .into_iter()
        .collect()
}

fn identities(store: &mut dyn Store, entity: &str) -> Vec<Value> {
    rows(store, entity)
        .into_iter()
        .map(|(identity, _)| identity)
        .collect()
}

fn fields(row: &Row) -> BTreeMap<&str, &Value> {
    row.fields
        .iter()
        .map(|(name, value)| (name.as_str(), value))
        .collect()
}

/// Two releases, `r1` and `r2`; `r1` holds the live records `k1` and `k2`, at version `v1`.
fn arrangement() -> Vec<(&'static str, Value)> {
    vec![
        (OPEN, json!({"release_id": "r1"})),
        (OPEN, json!({"release_id": "r2"})),
        (
            ADD_RECORD,
            json!({"record_id": "k1", "release_id": "r1", "version": "v1"}),
        ),
        (
            ADD_RECORD,
            json!({"record_id": "k2", "release_id": "r1", "version": "v1"}),
        ),
    ]
}

fn arrange(ir: &spindle_spec::EssIr, store: &mut dyn Store) {
    for (command, input) in arrangement() {
        run(ir, store, command, input);
    }
}

#[test]
fn an_affects_entry_over_an_input_list_creates_one_row_per_element() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);

        let published = run(
            &ir,
            store,
            PUBLISH,
            json!({"release_id": "r2", "contracts": [
                {"record_id": "n1", "version": "a"},
                {"record_id": "n2", "version": "b", "note": "second"},
                {"record_id": "n3", "version": "c"},
            ]}),
        );

        assert_eq!(published.outcome, "published", "{name}");
        assert_eq!(published.error, None, "{name}");
        assert_eq!(
            identities(store, RECORD),
            [
                json!("k1"),
                json!("k2"),
                json!("n1"),
                json!("n2"),
                json!("n3")
            ],
            "{name}: one row per element, beside the rows held"
        );
        for (record, version, note) in [
            ("n1", "a", None),
            ("n2", "b", Some("second")),
            ("n3", "c", None),
        ] {
            let row = held(store, RECORD, record);
            assert_eq!(row.state, "Live", "{name}: {record} rests in `initial`");
            let mut expected =
                BTreeMap::from([("release_id", json!("r2")), ("version", json!(version))]);
            if let Some(note) = note {
                expected.insert("note", json!(note));
            }
            assert_eq!(
                fields(&row),
                expected
                    .iter()
                    .map(|(name, value)| (*name, value))
                    .collect(),
                "{name}: {record} holds the entry's `sets:` and its element's members"
            );
        }
        assert_eq!(
            held(store, RELEASE, "r2").fields.get("last"),
            Some(&json!("published")),
            "{name}: the subject's own effect applies"
        );
        assert_eq!(
            published
                .events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["depot.release.Published"],
            "{name}: the outcome publishes its own event"
        );
    }
}

#[test]
fn an_affects_entry_over_an_empty_input_list_creates_no_row() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let before = rows(store, RECORD);

        let published = run(
            &ir,
            store,
            PUBLISH,
            json!({"release_id": "r1", "contracts": []}),
        );

        assert_eq!(published.outcome, "published", "{name}");
        assert_eq!(rows(store, RECORD), before, "{name}: no record is written");
        assert_eq!(
            held(store, RELEASE, "r1").fields.get("last"),
            Some(&json!("published")),
            "{name}: the subject's effect still applies"
        );
    }
}

#[test]
fn an_affects_entry_over_an_input_list_updates_the_rows_it_names() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let other = stored(store, RECORD, "k2");

        let published = run(
            &ir,
            store,
            PUBLISH,
            json!({"release_id": "r2", "contracts": [
                {"record_id": "k1", "version": "v2", "note": "moved"},
            ]}),
        );

        assert_eq!(published.outcome, "published", "{name}");
        assert_eq!(
            identities(store, RECORD),
            [json!("k1"), json!("k2")],
            "{name}: the held row is updated, and no second row is created"
        );
        let row = held(store, RECORD, "k1");
        assert_eq!(row.state, "Live", "{name}");
        assert_eq!(
            fields(&row),
            BTreeMap::from([
                ("note", &json!("moved")),
                ("release_id", &json!("r2")),
                ("version", &json!("v2")),
            ]),
            "{name}: the held row comes to hold the element's values"
        );
        assert_eq!(
            stored(store, RECORD, "k2"),
            other,
            "{name}: a row no element names is left as it was"
        );
    }
}

#[test]
fn an_affects_entry_over_an_input_list_moves_only_held_rows() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let other = stored(store, RECORD, "k2");

        let retired = run(
            &ir,
            store,
            RETIRE,
            json!({"release_id": "r1", "records": [{"record_id": "k1"}, {"record_id": "n9"}]}),
        );

        assert_eq!(retired.outcome, "retired", "{name}");
        assert_eq!(held(store, RECORD, "k1").state, "Retired", "{name}");
        assert_eq!(
            stored(store, RECORD, "k2"),
            other,
            "{name}: a held row no element names stays"
        );
        assert_eq!(
            identities(store, RECORD),
            [json!("k1"), json!("k2")],
            "{name}: an element naming no held row creates none"
        );

        let first = stored(store, RECORD, "k1");
        let again = run(
            &ir,
            store,
            RETIRE,
            json!({"release_id": "r1", "records": [{"record_id": "k1"}, {"record_id": "k2"}]}),
        );
        assert_eq!(again.outcome, "retired", "{name}");
        assert_eq!(
            stored(store, RECORD, "k1"),
            first,
            "{name}: a held row resting outside the move's `from` states is skipped"
        );
        assert_eq!(held(store, RECORD, "k2").state, "Retired", "{name}");
    }
}

#[test]
fn an_affects_entry_over_an_input_list_deletes_only_held_rows() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let before = rows(store, RECORD);

        let purged = run(
            &ir,
            store,
            PURGE,
            json!({"release_id": "r1", "records": [{"record_id": "k1"}, {"record_id": "n9"}]}),
        );

        assert_eq!(purged.outcome, "purged", "{name}");
        assert_eq!(
            rows(store, RECORD),
            before
                .into_iter()
                .filter(|(identity, _)| *identity != json!("k1"))
                .collect::<Vec<_>>(),
            "{name}: the held row listed is removed, nothing created, the rest unchanged"
        );
    }
}

#[test]
fn an_element_its_filter_does_not_select_is_skipped() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let held_k1 = stored(store, RECORD, "k1");

        let published = run(
            &ir,
            store,
            PUBLISH_STABLE,
            json!({"release_id": "r2", "skip": "beta", "contracts": [
                {"record_id": "n1", "version": "alpha"},
                {"record_id": "n2", "version": "beta"},
                {"record_id": "k1", "version": "beta"},
            ]}),
        );

        assert_eq!(published.outcome, "published", "{name}");
        assert_eq!(
            identities(store, RECORD),
            [json!("k1"), json!("k2"), json!("n1")],
            "{name}: only the element the filter selects creates its row"
        );
        assert_eq!(
            held(store, RECORD, "n1").fields.get("version"),
            Some(&json!("alpha")),
            "{name}"
        );
        assert_eq!(
            stored(store, RECORD, "k1"),
            held_k1,
            "{name}: a held row whose element the filter does not select is left as it was"
        );
    }
}

#[test]
fn an_element_naming_the_subject_is_skipped() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);

        let linked = run(
            &ir,
            store,
            LINK,
            json!({"release_id": "r1", "releases": [
                {"release_id": "r1"},
                {"release_id": "r2"},
                {"release_id": "r3"},
            ]}),
        );

        assert_eq!(linked.outcome, "linked", "{name}");
        let subject = held(store, RELEASE, "r1");
        assert_eq!(
            fields(&subject),
            BTreeMap::from([("last", &json!("linked"))]),
            "{name}: the subject holds what its own effect writes, and no link to itself"
        );
        assert_eq!(
            held(store, RELEASE, "r2").fields.get("linked_to"),
            Some(&json!("r1")),
            "{name}: a held release listed is linked"
        );
        let created = held(store, RELEASE, "r3");
        assert_eq!(created.state, "Open", "{name}: r3 is created in `initial`");
        assert_eq!(
            fields(&created),
            BTreeMap::from([("linked_to", &json!("r1"))]),
            "{name}"
        );
    }
}

/// Every value of `rows` read as ESS's [`Node`]: a `Decimal` identity the interpreter holds as
/// `1.0` the kernel holds as `1`, one number either way.
fn as_nodes(rows: Rows) -> Vec<Vec<BTreeMap<String, Node>>> {
    rows.into_iter()
        .map(|view| {
            view.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|(field, value)| {
                            let node = serde_json::from_value(value.clone())
                                .unwrap_or_else(|_| panic!("{field} holds a node, not {value}"));
                            (field, node)
                        })
                        .collect()
                })
                .collect()
        })
        .collect()
}

/// An [`Answer`] with every event payload value read as ESS's [`Node`].
type NodeAnswer = (
    String,
    Option<String>,
    Vec<(String, BTreeMap<String, Node>)>,
);

/// Each answer with every event payload value read as ESS's [`Node`]: the interpreter publishes a
/// `Decimal` given as `1` as `1.0`, one number either way.
fn answers_as_nodes(answers: Vec<Answer>) -> Vec<NodeAnswer> {
    answers
        .into_iter()
        .map(|(outcome, error, events)| {
            let events = events
                .into_iter()
                .map(|(event, payload)| {
                    let payload = payload
                        .into_iter()
                        .map(|(field, value)| {
                            let node = serde_json::from_value(value.clone())
                                .unwrap_or_else(|_| panic!("{field} holds a node, not {value}"));
                            (field, node)
                        })
                        .collect();
                    (event, payload)
                })
                .collect();
            (outcome, error, events)
        })
        .collect()
}

#[test]
fn affects_over_an_input_list_agrees_with_the_interpreter() {
    let ir = ir();
    let mut steps = arrangement();
    steps.extend([
        (ADD_BIN, json!({"slot": 1, "label": "one"})),
        (
            PUBLISH,
            json!({"release_id": "r1", "contracts": [
                {"record_id": "n1", "version": "a"},
                {"record_id": "n2", "version": "b", "note": "second"},
                {"record_id": "n3", "version": "c"},
            ]}),
        ),
        (PUBLISH, json!({"release_id": "r1", "contracts": []})),
        (
            PUBLISH,
            json!({"release_id": "r2", "contracts": [
                {"record_id": "k1", "version": "v2", "note": "moved"},
                {"record_id": "n4", "version": "d"},
            ]}),
        ),
        (
            PUBLISH,
            json!({"release_id": "r1", "contracts": [
                {"record_id": "k2", "version": "x"},
                {"record_id": "k2", "version": "y"},
            ]}),
        ),
        (
            PUBLISH_STABLE,
            json!({"release_id": "r1", "skip": "beta", "contracts": [
                {"record_id": "n5", "version": "alpha"},
                {"record_id": "n6", "version": "beta"},
                {"record_id": "k2", "version": "beta"},
            ]}),
        ),
        (
            RETIRE,
            json!({"release_id": "r1", "records": [{"record_id": "k1"}, {"record_id": "n9"}]}),
        ),
        (
            RETIRE,
            json!({"release_id": "r1", "records": [{"record_id": "k1"}, {"record_id": "k2"}]}),
        ),
        (
            PURGE,
            json!({"release_id": "r1", "records": [{"record_id": "n1"}, {"record_id": "n8"}]}),
        ),
        (
            PUBLISH,
            json!({"release_id": "r1", "contracts": [{"record_id": "n1", "version": "e"}]}),
        ),
        (
            LINK,
            json!({"release_id": "r1", "releases": [
                {"release_id": "r1"},
                {"release_id": "r2"},
                {"release_id": "r3"},
            ]}),
        ),
        (
            FILL,
            json!({"release_id": "r1", "bins": [
                {"slot": 1.0, "label": "a"},
                {"slot": 2, "label": "b"},
                {"slot": 1, "label": "c"},
            ]}),
        ),
        (
            FILL,
            json!({"release_id": "r2", "bins": [
                {"slot": 2.5, "label": "x"},
                {"slot": 2.5, "label": "y"},
            ]}),
        ),
    ]);
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes[4..],
        [
            "added",
            "published",
            "published",
            "published",
            "duplicated",
            "published",
            "retired",
            "retired",
            "purged",
            "published",
            "linked",
            "filled",
            "filled",
        ],
        "the steps create, update, move and remove rows, skip elements and repeat a bin"
    );
    let (answers, rows): (Vec<Answer>, Rows) =
        spindled_with(&VIEWS, &steps, &mut |store, command, input| {
            run(&ir, store, command, input)
        });
    assert_eq!(
        answers_as_nodes(answers),
        answers_as_nodes(ess_answers),
        "outcomes, errors and events agree"
    );
    assert_eq!(
        as_nodes(rows),
        as_nodes(ess_rows),
        "the stored releases, records and bins agree with ESS's view rows"
    );
}
