//! A view field read through a reference (`story:view-related-fields`): a field declared
//! `field: {related: {via: <reference>, field: <field>}}` answers what the row the reference names
//! holds now, and nothing where the reference is empty, names no row or names a row since removed,
//! as ESS's interpreter answers it.
//!
//! `tests/fixtures/related` declares two components: `catalogue` keeps resources, each with a
//! name and an optional label, that are renamed, retired and dropped; `grants` keeps grants, each
//! naming a resource or none. Every view over a grant reads the resource's name, and `Grants` its
//! label, state and identity too.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_kernel::{Change, MemoryStore, Removal, Store, StoreError, Stored};

const GRANTS: &str = "access.grants.Grants";
const RESOURCE: &str = "access.resources.Resource";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/related")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"related\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// The commands every case sends, in order: `r1` added and renamed, `r2` added without a label
/// and retired, `r3` added and dropped; `g1` and `g2` on `r1`, `g3` on `r2`, `g4` on none, `g5`
/// on `r3` (dropped after), `g6` on `r9`, which no resource ever carried.
fn script() -> Vec<(&'static str, Value)> {
    vec![
        (
            "access.resources.AddResource",
            json!({"resource_id": "r1", "name": "printer", "label": "floor 2"}),
        ),
        (
            "access.resources.AddResource",
            json!({"resource_id": "r2", "name": "scanner"}),
        ),
        (
            "access.resources.AddResource",
            json!({"resource_id": "r3", "name": "plotter"}),
        ),
        (
            "access.grants.IssueGrant",
            json!({"grant_id": "g1", "holder": "ada", "resource_id": "r1"}),
        ),
        (
            "access.grants.IssueGrant",
            json!({"grant_id": "g2", "holder": "bob", "resource_id": "r1"}),
        ),
        (
            "access.grants.IssueGrant",
            json!({"grant_id": "g3", "holder": "ada", "resource_id": "r2"}),
        ),
        (
            "access.grants.IssueGrant",
            json!({"grant_id": "g4", "holder": "cy"}),
        ),
        (
            "access.grants.IssueGrant",
            json!({"grant_id": "g5", "holder": "dee", "resource_id": "r3"}),
        ),
        (
            "access.grants.IssueGrant",
            json!({"grant_id": "g6", "holder": "ada", "resource_id": "r9"}),
        ),
        (
            "access.resources.RenameResource",
            json!({"resource_id": "r1", "name": "colour printer"}),
        ),
        (
            "access.resources.RetireResource",
            json!({"resource_id": "r2"}),
        ),
        (
            "access.resources.DropResource",
            json!({"resource_id": "r3"}),
        ),
    ]
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// Every view of the fixture, by name.
fn views() -> Vec<String> {
    spindle_spec::load(&fixture())
        .expect("the related fixture compiles")
        .ir
        .views()
        .keys()
        .map(ToString::to_string)
        .collect()
}

/// A host over memory with [`script`] run on it.
fn host(state: &Path) -> Host {
    let ir = spindle_spec::load(&fixture())
        .expect("the related fixture compiles")
        .ir;
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state)
        .expect("the host admits the related fixture");
    run(&mut host);
    host
}

fn run(host: &mut Host) {
    for (command, input) in script() {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{command} {input} runs on the host: {error}"));
    }
}

/// ESS's interpreter with [`script`] run on it.
fn interpreter() -> Interpreted {
    let ir = spindle_spec::load(&fixture())
        .expect("the related fixture compiles")
        .ir;
    let interpreter = Interpreted::for_model(ir);
    let correlation = CorrelationId::new("related").expect("a correlation");
    for (command, input) in script() {
        let fields: BTreeMap<String, Node> =
            serde_json::from_value(input.clone()).expect("the input as ESS values");
        interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{command} {input} runs on the interpreter: {error:?}"));
    }
    interpreter
}

/// The rows the interpreter answers for `view`, in its order, or `None` where it refuses the read.
/// A member it answers `null` is left out: none of the fixture's view fields declares a
/// `presence:` policy, and for such a field ESS reads a member left out and one sent as `null` as
/// the one absent value.
fn interpreted(interpreter: &Interpreted, view: &str) -> Option<Vec<Value>> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: BTreeMap::new(),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("related").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some(
        answer
            .rows
            .iter()
            .map(|row| {
                Value::Object(
                    row.iter()
                        .filter(|(_, value)| **value != Node::Null)
                        .map(|(name, value)| {
                            (
                                name.clone(),
                                serde_json::to_value(value).expect("a value as JSON"),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

/// The row of `rows` whose `grant_id` is `grant`.
fn grant<'r>(rows: &'r [Value], grant: &str) -> &'r Value {
    rows.iter()
        .find(|row| row["grant_id"] == grant)
        .unwrap_or_else(|| panic!("a row for {grant} in {rows:?}"))
}

/// Each grant row carries what the resource it names holds now: `g1` and `g2` the renamed
/// resource's name and label, `g3` the retired resource's name and state. A grant naming no
/// resource (`g4`), one dropped since (`g5`) or one never added (`g6`) carries none of them.
#[test]
fn each_row_carries_what_its_resource_holds_now() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(state.path());
    let rows = host
        .list(GRANTS)
        .unwrap_or_else(|error| panic!("{GRANTS} answers: {error}"))
        .into_iter()
        .map(Value::Object)
        .collect::<Vec<_>>();

    for id in ["g1", "g2"] {
        assert_eq!(
            grant(&rows, id),
            &json!({"grant_id": id, "holder": if id == "g1" { "ada" } else { "bob" },
                    "resource_id": "r1", "resource_name": "colour printer",
                    "resource_label": "floor 2", "resource_state": "Active",
                    "resource_key": "r1"}),
        );
    }
    assert_eq!(
        grant(&rows, "g3"),
        &json!({"grant_id": "g3", "holder": "ada", "resource_id": "r2",
                "resource_name": "scanner", "resource_state": "Retired", "resource_key": "r2"}),
        "a field the resource holds nothing for is absent"
    );
    assert_eq!(
        grant(&rows, "g4"),
        &json!({"grant_id": "g4", "holder": "cy"})
    );
    assert_eq!(
        grant(&rows, "g5"),
        &json!({"grant_id": "g5", "holder": "dee", "resource_id": "r3"}),
        "the dropped resource is read as no row"
    );
    assert_eq!(
        grant(&rows, "g6"),
        &json!({"grant_id": "g6", "holder": "ada", "resource_id": "r9"})
    );

    assert_eq!(
        host.view(GRANTS, &json!("g1"))
            .unwrap_or_else(|error| panic!("{GRANTS} answers g1: {error}"))
            .map(Value::Object)
            .as_ref(),
        Some(grant(&rows, "g1")),
        "one row read by identity carries the resource's fields as the list does"
    );
    assert_eq!(
        host.view(GRANTS, &json!("g5"))
            .unwrap_or_else(|error| panic!("{GRANTS} answers g5: {error}"))
            .map(Value::Object)
            .as_ref(),
        Some(grant(&rows, "g5")),
    );
}

/// Every view of the fixture answers the rows ESS's interpreter answers, in its order.
#[test]
fn related_fields_agree_with_the_interpreter() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(state.path());
    let interpreter = interpreter();
    let mut differences = Vec::new();
    for view in views() {
        let served = host
            .list(&view)
            .map(|rows| rows.into_iter().map(Value::Object).collect::<Vec<_>>())
            .ok();
        let interpreted = interpreted(&interpreter, &view);
        if interpreted.is_none() {
            differences.push(format!("{view}: the interpreter refuses the read"));
        }
        if served != interpreted {
            differences.push(format!(
                "{view}: host {served:?}, interpreter {interpreted:?}"
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "views whose rows differ from ESS's interpreter: {differences:#?}"
    );
}

/// What a store was asked, by entity: each identity loaded, in order, and each scan.
#[derive(Default)]
struct Asked {
    loads: Vec<(String, Value)>,
    scans: Vec<String>,
}

/// A memory store that notes every load and scan it answers.
#[derive(Clone, Default)]
struct Counted {
    store: Arc<Mutex<MemoryStore>>,
    asked: Arc<Mutex<Asked>>,
}

impl Store for Counted {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.asked
            .lock()
            .expect("the tally")
            .loads
            .push((entity.to_owned(), identity.clone()));
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.lock().expect("the store").remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.asked
            .lock()
            .expect("the tally")
            .scans
            .push(entity.to_owned());
        self.store.lock().expect("the store").scan(entity)
    }
}

/// An environment whose only store is `supplied`, which the host is given one of per component.
fn supplied() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"supplied\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"supplied\"]\n",
        "\"store.ephemeral\" = [\"supplied\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the supplied environment parses")
}

/// A list reads each resource a grant names by its identity, once per distinct identity, from
/// the store of the component that owns resources, and never scans resources: six grants name
/// four distinct resources (`r1` twice) and one names none, so the list loads four rows.
#[test]
fn a_list_loads_each_distinct_resource_once_and_scans_none() {
    let ir = spindle_spec::load(&fixture())
        .expect("the related fixture compiles")
        .ir;
    let catalogue = Counted::default();
    let stores = Stores::from([
        (
            "catalogue".to_owned(),
            Box::new(catalogue.clone()) as Box<dyn Store>,
        ),
        (
            "grants".to_owned(),
            Box::new(Counted::default()) as Box<dyn Store>,
        ),
    ]);
    let mut host = Host::supplied(ir, &supplied(), Arc::new(ManualClock::new()), None, stores)
        .unwrap_or_else(|error| panic!("the host opens on the supplied stores: {error}"));
    run(&mut host);
    *catalogue.asked.lock().expect("the tally") = Asked::default();

    let rows = host
        .list(GRANTS)
        .unwrap_or_else(|error| panic!("{GRANTS} answers: {error}"));
    assert_eq!(rows.len(), 6);

    let asked = catalogue.asked.lock().expect("the tally");
    assert!(
        asked.scans.is_empty(),
        "no scan of the catalogue: {:?}",
        asked.scans
    );
    let loaded = asked
        .loads
        .iter()
        .map(|(entity, identity)| {
            assert_eq!(entity, RESOURCE);
            identity.as_str().expect("a resource id").to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        loaded.iter().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["r1", "r2", "r3", "r9"].map(str::to_owned)),
    );
    assert_eq!(loaded.len(), 4, "each resource loaded once: {loaded:?}");
}

/// A view with a field read through a reference is read on the host's thread, where the row a
/// reference names is loaded by its identity: the reader beside it does not claim it.
#[test]
fn the_reader_leaves_a_related_view_to_the_hosts_thread() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = host(state.path());
    assert!(spindle_host::reads(host.ir(), GRANTS).is_ok());
    assert!(!host.reader().reads(GRANTS));
}
