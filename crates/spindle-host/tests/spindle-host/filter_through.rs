//! A view filter read through a reference, or comparing a field with another field or a
//! parameter (`story:view-filter-through-reference`): `person_id.state == Active`,
//! `person_id.name == holder`, `reviewer_id == person_id`, `started_at > person_id.joined_at`,
//! `person_id.team == param.team` and the rest are answered as ESS's interpreter answers them, a
//! row whose reference names no row hidden where the filter is unknown only for that
//! (`absent_navigation`).
//!
//! `tests/fixtures/filter-through` declares two components: `directory` keeps people, each with a
//! name, a team, an optional nickname and the instant they joined, who leave and are removed;
//! `work` keeps tasks, each held under a name by a person and reviewed by one or none.

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
use spindle_kernel::{Change, MemoryStore, Removal, SqliteStore, Store, StoreError, Stored};

const PERSON: &str = "org.people.Person";
const WITH_ACTIVE_PERSON: &str = "org.tasks.WithActivePerson";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filter-through")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the filter-through fixture compiles")
        .ir
}

/// The commands every case sends, in order. `p2` leaves and `p4` is removed after the tasks are
/// opened; `t5` names `p9`, whom no one ever added, as its person and its reviewer; `t3` has no
/// reviewer; `t7` is closed.
fn script() -> Vec<(&'static str, Value)> {
    vec![
        (
            "org.people.AddPerson",
            json!({"person_id": "p1", "name": "ada", "team": "red", "nickname": "A",
                   "joined_at": "2026-01-01T00:00:00Z"}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p2", "name": "bob", "team": "blue",
                   "joined_at": "2026-03-01T00:00:00Z"}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p3", "name": "cy", "team": "red", "nickname": "C",
                   "joined_at": "2026-02-01T01:00:00+01:00"}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p4", "name": "dee", "team": "blue",
                   "joined_at": "2026-01-01T00:00:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t1", "holder": "ada", "person_id": "p1", "reviewer_id": "p2",
                   "started_at": "2026-02-01T00:00:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t2", "holder": "bob", "person_id": "p1", "reviewer_id": "p1",
                   "started_at": "2025-12-01T00:00:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t3", "holder": "bob", "person_id": "p2",
                   "started_at": "2026-04-01T00:00:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t4", "holder": "dee", "person_id": "p4", "reviewer_id": "p3",
                   "started_at": "2026-02-01T00:00:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t5", "holder": "eve", "person_id": "p9", "reviewer_id": "p9",
                   "started_at": "2026-02-01T00:00:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t6", "holder": "cy", "person_id": "p3", "reviewer_id": "p3",
                   "started_at": "2026-02-01T00:30:00Z"}),
        ),
        (
            "org.tasks.OpenTask",
            json!({"task_id": "t7", "holder": "ada", "person_id": "p1", "reviewer_id": "p3",
                   "started_at": "2026-05-01T00:00:00Z"}),
        ),
        ("org.tasks.CloseTask", json!({"task_id": "t7"})),
        ("org.people.LeavePerson", json!({"person_id": "p2"})),
        ("org.people.RemovePerson", json!({"person_id": "p4"})),
    ]
}

/// Every read the cases make: each view of the fixture with each set of parameters it is read
/// with, a view without parameters once with none.
fn reads() -> Vec<(String, Value)> {
    let mut reads = Vec::new();
    for (name, view) in ir().views() {
        let name = name.to_string();
        match view.params.first().map(|param| param.name.as_str()) {
            None => reads.push((name, json!({}))),
            Some("viewer") => {
                for viewer in ["p1", "p2", "p3", "p9"] {
                    reads.push((name.clone(), json!({"viewer": viewer})));
                }
            }
            Some("team") => {
                for team in ["red", "blue", "green"] {
                    reads.push((name.clone(), json!({"team": team})));
                }
            }
            Some(other) => panic!("{name} reads a parameter no case gives: {other}"),
        }
    }
    reads
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// An environment whose only store is `supplied`, which the host is given one of per component.
fn supplied() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"filter-through\"\n",
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

/// A host on `stores`, one per component, with [`script`] run on it.
fn host(stores: Stores) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        stores,
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied stores: {error}"));
    for (command, input) in script() {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{command} {input} runs on the host: {error}"));
    }
    host
}

/// One store per component of the fixture: memory, or a SQLite file in `state`.
fn stores(store: &str, state: &Path) -> Stores {
    let open = |component: &str| -> Box<dyn Store> {
        match store {
            "memory" => Box::new(MemoryStore::default()),
            _ => Box::new(
                SqliteStore::open(&state.join(format!("{component}.sqlite")))
                    .expect("the SQLite file opens"),
            ),
        }
    };
    Stores::from([
        ("directory".to_owned(), open("directory")),
        ("work".to_owned(), open("work")),
    ])
}

/// ESS's interpreter with [`script`] run on it.
fn interpreter() -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("filter-through").expect("a correlation");
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

/// The rows the interpreter answers for `view` with `params`, in its order, or `None` where it
/// refuses the read. A member it answers `null` is left out: no field of the fixture's views
/// declares a `presence:` policy, and for such a field ESS reads a member left out and one sent as
/// `null` as the one absent value.
fn interpreted(interpreter: &Interpreted, view: &str, params: &Value) -> Option<Vec<Value>> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: serde_json::from_value(params.clone()).expect("the parameters as ESS values"),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("filter-through").expect("a correlation"),
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

fn rows(answer: Vec<Map<String, Value>>) -> Vec<Value> {
    answer.into_iter().map(Value::Object).collect()
}

/// The task ids of `rows`, in their order.
fn tasks(rows: &[Value]) -> Vec<&str> {
    rows.iter()
        .map(|row| row["task_id"].as_str().expect("a task id"))
        .collect()
}

/// Every read of the fixture answers, on the memory and on the SQLite store, on the host's thread
/// and through its reader, the rows ESS's interpreter answers, which answers every one of them.
#[test]
fn filters_through_a_reference_agree_with_the_interpreter() {
    let interpreter = interpreter();
    for store in ["memory", "sqlite"] {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(stores(store, state.path()));
        let reader = host.reader();
        let mut differences = Vec::new();
        for (view, params) in reads() {
            let expected = interpreted(&interpreter, &view, &params);
            if expected.is_none() {
                differences.push(format!("{view} {params}: the interpreter refuses the read"));
            }
            let served = host
                .list_with(&view, &object(params.clone()))
                .map(rows)
                .map_err(|error| error.to_string());
            if served.as_ref().ok() != expected.as_ref() {
                differences.push(format!(
                    "{store} {view} {params}: host {served:?}, interpreter {expected:?}"
                ));
            }
            if !reader.reads(&view) {
                differences.push(format!("{store} {view}: the reader does not answer it"));
            }
            let beside = reader
                .list_with(&view, &object(params.clone()))
                .map(rows)
                .map_err(|error| error.to_string());
            if beside.as_ref().ok() != expected.as_ref() {
                differences.push(format!(
                    "{store} {view} {params}: reader {beside:?}, interpreter {expected:?}"
                ));
            }
        }
        assert!(
            differences.is_empty(),
            "reads whose rows differ from ESS's interpreter: {differences:#?}"
        );
    }
}

/// The rows the filter reading `person_id.state` holds, spelled out: the tasks of the active
/// people `p1` and `p3`, not `t3`, whose person left, and neither `t4` nor `t5`, whose person is
/// removed or was never added (`absent_navigation`), each with its person's name.
#[test]
fn a_filter_on_the_referenced_state_keeps_the_rows_of_active_people() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(stores("memory", state.path()));
    let answered = rows(
        host.list(WITH_ACTIVE_PERSON)
            .unwrap_or_else(|error| panic!("{WITH_ACTIVE_PERSON} answers: {error}")),
    );
    assert_eq!(tasks(&answered), ["t1", "t2", "t6", "t7"]);
    assert_eq!(
        answered[0],
        json!({"task_id": "t1", "holder": "ada", "person_name": "ada"})
    );
}

/// A view read by identity holds a row through a reference as the list holds it: the row the list
/// answers, and none for a task the list leaves out, `t4` and `t5` among them.
#[test]
fn a_row_read_by_identity_is_held_as_the_list_holds_it() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(stores("memory", state.path()));
    for (view, params) in reads() {
        if params != json!({}) {
            continue;
        }
        let listed = rows(
            host.list(&view)
                .unwrap_or_else(|error| panic!("{view} answers: {error}")),
        );
        for task in ["t1", "t2", "t3", "t4", "t5", "t6", "t7"] {
            let one = host
                .view(&view, &json!(task))
                .unwrap_or_else(|error| panic!("{view} answers {task}: {error}"))
                .map(Value::Object);
            assert_eq!(
                one.as_ref(),
                listed.iter().find(|row| row["task_id"] == task),
                "{view}: {task} by identity"
            );
        }
    }
}

/// What a store was asked: each identity loaded, by entity, in order, and each entity scanned.
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

/// The people the tasks of [`script`] name in the fields `vias`, each once.
fn named(vias: &[String]) -> BTreeSet<String> {
    script()
        .into_iter()
        .filter(|(command, _)| *command == "org.tasks.OpenTask")
        .flat_map(|(_, input)| {
            vias.iter()
                .filter_map(|via| input.get(via).and_then(Value::as_str).map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every read loads each person a task names in a reference the view reads through, by its
/// identity, once however many tasks and references name them, from the directory's store, and
/// never scans people. The seven tasks name `p1`, `p2`, `p3`, `p4` and `p9` in `person_id` and
/// `reviewer_id`; a view that navigates only `reviewer_id` (`ForViewer` compares `person_id`
/// itself) loads only the reviewers, `p1`, `p2`, `p3` and `p9`, and one that navigates none
/// (`SelfReviewed`) loads no person.
#[test]
fn a_filter_loads_each_distinct_person_once_and_scans_none() {
    let directory = Counted::default();
    let mut host = host(Stores::from([
        (
            "directory".to_owned(),
            Box::new(directory.clone()) as Box<dyn Store>,
        ),
        (
            "work".to_owned(),
            Box::new(Counted::default()) as Box<dyn Store>,
        ),
    ]));
    let ir = ir();
    assert_eq!(
        named(&["person_id".to_owned(), "reviewer_id".to_owned()]),
        BTreeSet::from(["p1", "p2", "p3", "p4", "p9"].map(str::to_owned)),
    );
    assert_eq!(
        named(&["reviewer_id".to_owned()]),
        BTreeSet::from(["p1", "p2", "p3", "p9"].map(str::to_owned)),
    );
    for (view, params) in reads() {
        let vias = ir
            .views()
            .iter()
            .find(|(name, _)| name.to_string() == view)
            .map(|(_, declared)| {
                declared
                    .references
                    .iter()
                    .map(|reference| reference.via.clone())
                    .collect::<Vec<_>>()
            })
            .expect("a view of the fixture");
        let expected = named(&vias);
        *directory.asked.lock().expect("the tally") = Asked::default();
        host.list_with(&view, &object(params.clone()))
            .unwrap_or_else(|error| panic!("{view} {params} answers: {error}"));
        let asked = directory.asked.lock().expect("the tally");
        assert!(
            asked.scans.is_empty(),
            "{view} {params}: no scan of the directory: {:?}",
            asked.scans
        );
        let loaded = asked
            .loads
            .iter()
            .map(|(entity, identity)| {
                assert_eq!(entity, PERSON, "{view} {params}");
                identity.as_str().expect("a person id").to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            loaded.iter().cloned().collect::<BTreeSet<_>>(),
            expected,
            "{view} {params} through {vias:?}: {loaded:?}"
        );
        assert_eq!(
            loaded.len(),
            expected.len(),
            "{view} {params}: each person once: {loaded:?}"
        );
    }
}
