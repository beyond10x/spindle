//! The caller a command is sent as (ess/16): `{caller: <attribute>}` as a `sets:` or `payload:`
//! source, `caller.<attribute>` as an operand of `when:` and `when_subject:`, and a branch the
//! caller decides, as ESS's interpreter reads them (`interpret::execute::caller`).
//!
//! The fixture `tests/fixtures/caller` declares the actor `Writer`, whose credential carries
//! `owner_id`. `CreateNotebook` stores and publishes the creator's `owner_id`; `EditNotebook` and
//! `ShareNotebook` refuse with `not-yours` when `owner_id != caller.owner_id`, and
//! `ShareNotebook` refuses with `with-self` when `with == caller.owner_id`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use ess_compiler::refs::{ActorRef, CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Executed, Invocation, MemoryStore, NoExternals, Store, UuidV4, execute_as,
};

const NOTEBOOK: &str = "quill.notebook.Notebook";
const CREATE: &str = "quill.notebook.CreateNotebook";
const EDIT: &str = "quill.notebook.EditNotebook";
const SHARE: &str = "quill.notebook.ShareNotebook";
const WRITER: &str = "quill.notebook.Writer";
const CARD: &str = "quill.notebook.NotebookCard";

const NOTEBOOK_ID: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
const OWNER: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const OTHER: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/caller");
    spindle_spec::load(&root)
        .expect("the caller fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn writer(owner: &str) -> Map<String, Value> {
    object(json!({"owner_id": owner}))
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    command: &str,
    input: Value,
    caller: Option<&Map<String, Value>>,
) -> Result<Executed, ExecuteError> {
    execute_as(
        ir,
        store,
        &mut NoExternals,
        &mut UuidV4,
        &SystemTime::now,
        &Invocation {
            command,
            input: &object(input),
            actor: caller.is_some().then_some(WRITER),
            caller,
        },
    )
}

fn create(ir: &spindle_spec::EssIr, store: &mut MemoryStore, owner: &str) -> Executed {
    send(
        ir,
        store,
        CREATE,
        json!({"notebook_id": NOTEBOOK_ID, "title": "field notes"}),
        Some(&writer(owner)),
    )
    .expect("the notebook is created")
}

fn row(store: &mut MemoryStore) -> Option<BTreeMap<String, Value>> {
    store
        .load(NOTEBOOK, &json!(NOTEBOOK_ID))
        .expect("the store reads")
        .map(|stored| stored.row.fields)
}

#[test]
fn a_caller_value_is_stored_and_published() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let created = create(&ir, &mut store, OWNER);
    assert_eq!(created.outcome, "created");
    assert_eq!(created.error, None);
    assert_eq!(created.events.len(), 1, "{:?}", created.events);
    assert_eq!(
        created.events[0].payload,
        object(json!({"notebook_id": NOTEBOOK_ID, "owner_id": OWNER})),
        "the event publishes the caller's `owner_id`"
    );
    assert_eq!(
        row(&mut store).and_then(|fields| fields.get("owner_id").cloned()),
        Some(json!(OWNER)),
        "the notebook stores the caller's `owner_id`"
    );
}

#[test]
fn a_branch_the_caller_decides_refuses_another_caller() {
    let ir = ir();
    let command = ir
        .commands()
        .iter()
        .find_map(|(name, command)| (name.to_string() == EDIT).then_some(command))
        .expect("the fixture declares EditNotebook");
    assert!(
        command
            .outcomes
            .iter()
            .any(|outcome| outcome.decided_by_caller),
        "`not-yours` is a branch the caller decides"
    );
    spindle_kernel::runs(&ir, command).expect("the kernel runs a branch the caller decides");

    let mut store = MemoryStore::default();
    create(&ir, &mut store, OWNER);
    let retitle = json!({"notebook_id": NOTEBOOK_ID, "title": "fair copy"});

    let refused = send(&ir, &mut store, EDIT, retitle.clone(), Some(&writer(OTHER)))
        .expect("the edit by another writer is answered");
    assert_eq!(refused.outcome, "not-yours");
    assert_eq!(
        refused.error.as_ref().map(|error| error.name.as_str()),
        Some("quill.notebook.NotYours")
    );
    assert!(refused.events.is_empty(), "{:?}", refused.events);
    assert_eq!(
        row(&mut store).and_then(|fields| fields.get("title").cloned()),
        Some(json!("field notes")),
        "a refused edit changes nothing"
    );

    let edited =
        send(&ir, &mut store, EDIT, retitle, Some(&writer(OWNER))).expect("the owner's edit runs");
    assert_eq!(edited.outcome, "edited");
    assert_eq!(edited.error, None);
    assert_eq!(
        row(&mut store).and_then(|fields| fields.get("title").cloned()),
        Some(json!("fair copy")),
        "the owner retitles the notebook"
    );
}

#[test]
fn a_command_sent_without_its_caller_is_refused_before_anything_is_written() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let created = send(
        &ir,
        &mut store,
        CREATE,
        json!({"notebook_id": NOTEBOOK_ID, "title": "field notes"}),
        None,
    );
    let Err(error) = created else {
        panic!("a creation reading `{{caller: owner_id}}` sent as no caller runs: {created:?}")
    };
    assert_eq!(error.kind(), "invalid_input", "{error}");
    assert!(error.to_string().contains("owner_id"), "{error}");
    assert_eq!(row(&mut store), None, "nothing is created");

    create(&ir, &mut store, OWNER);
    let before = row(&mut store);
    for (command, input) in [
        (
            EDIT,
            json!({"notebook_id": NOTEBOOK_ID, "title": "fair copy"}),
        ),
        (SHARE, json!({"notebook_id": NOTEBOOK_ID, "with": OTHER})),
    ] {
        let sent = send(&ir, &mut store, command, input, None);
        assert!(
            sent.is_err(),
            "{command} guarded on `caller.owner_id` sent as no caller selects no branch: {sent:?}"
        );
        assert_eq!(row(&mut store), before, "{command} writes nothing");
    }

    let mut undeclared = writer(OWNER);
    undeclared.insert("region".to_owned(), json!("north"));
    let sent = send(
        &ir,
        &mut store,
        EDIT,
        json!({"notebook_id": NOTEBOOK_ID, "title": "fair copy"}),
        Some(&undeclared),
    );
    let Err(error) = sent else {
        panic!("a caller carrying an attribute no actor declares is sent: {sent:?}")
    };
    assert_eq!(error.kind(), "invalid_input", "{error}");
    assert!(error.to_string().contains("region"), "{error}");
    let sent = send(
        &ir,
        &mut store,
        EDIT,
        json!({"notebook_id": NOTEBOOK_ID, "title": "fair copy"}),
        Some(&writer("not a uuid")),
    );
    assert!(
        matches!(sent, Err(ExecuteError::InvalidInput(_))),
        "a caller value its attribute's type does not admit is refused: {sent:?}"
    );
    let sent = execute_as(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &SystemTime::now,
        &Invocation {
            command: EDIT,
            input: &object(json!({"notebook_id": NOTEBOOK_ID, "title": "fair copy"})),
            actor: None,
            caller: Some(&writer(OWNER)),
        },
    );
    assert!(
        matches!(sent, Err(ExecuteError::InvalidInput(_))),
        "caller attributes sent as no actor are refused, as ESS refuses them: {sent:?}"
    );
    assert_eq!(row(&mut store), before, "a refused caller writes nothing");
}

/// One command step: what is sent, and as whom.
struct Step {
    command: &'static str,
    input: Value,
    caller: Option<&'static str>,
}

/// What one step answered: the outcome, the declared error, and the events by name and payload.
type Answer = (String, Option<String>, Vec<(String, Map<String, Value>)>);

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

/// What ESS's interpreter answers for each step, and the `NotebookCard` rows after the last.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Vec<Map<String, Value>>) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "quill.notebook/authored/caller"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("caller").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for step in steps {
        let input = object(step.input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let executed = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(step.command.parse().expect("a command name")),
                actor: Some(ActorRef::new(WRITER.parse().expect("an actor name"))),
                caller: step.caller.map(|owner| {
                    BTreeMap::from([("owner_id".to_owned(), Node::Text(owner.to_owned()))])
                }),
                input,
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("the interpreter runs {}: {error}", step.command));
        answers.push((
            executed
                .outcome
                .map(|outcome| outcome.outcome.as_str().to_owned())
                .unwrap_or_default(),
            executed.error.map(|error| error.error.to_string()),
            executed
                .direct_events
                .iter()
                .map(|event| {
                    (
                        event.event.to_string(),
                        event
                            .payload
                            .iter()
                            .map(|(field, node)| (field.clone(), json_of(node)))
                            .collect(),
                    )
                })
                .collect(),
        ));
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(CARD.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter reads the view")
        .rows
        .into_iter()
        .map(|row| {
            row.iter()
                .map(|(field, node)| (field.clone(), json_of(node)))
                .collect()
        })
        .collect();
    (answers, rows)
}

/// What the kernel answers for each step, and the stored notebook as the view projects it.
fn spindled(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Vec<Map<String, Value>>) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    for step in steps {
        let caller = step.caller.map(writer);
        let executed = send(
            ir,
            &mut store,
            step.command,
            step.input.clone(),
            caller.as_ref(),
        )
        .unwrap_or_else(|error| panic!("the kernel runs {}: {error}", step.command));
        answers.push((
            executed.outcome,
            executed.error.map(|error| error.name),
            executed
                .events
                .into_iter()
                .map(|event| (event.name, event.payload))
                .collect(),
        ));
    }
    let rows = store
        .scan(NOTEBOOK)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("notebook_id".to_owned(), identity);
            row
        })
        .collect();
    (answers, rows)
}

#[test]
fn caller_attributes_agree_with_the_interpreter() {
    let ir = ir();
    let steps = [
        Step {
            command: CREATE,
            input: json!({"notebook_id": NOTEBOOK_ID, "title": "field notes"}),
            caller: Some(OWNER),
        },
        Step {
            command: EDIT,
            input: json!({"notebook_id": NOTEBOOK_ID, "title": "stolen"}),
            caller: Some(OTHER),
        },
        Step {
            command: SHARE,
            input: json!({"notebook_id": NOTEBOOK_ID, "with": OWNER}),
            caller: Some(OWNER),
        },
        Step {
            command: SHARE,
            input: json!({"notebook_id": NOTEBOOK_ID, "with": OWNER}),
            caller: Some(OTHER),
        },
        Step {
            command: EDIT,
            input: json!({"notebook_id": NOTEBOOK_ID, "title": "fair copy"}),
            caller: Some(OWNER),
        },
        Step {
            command: SHARE,
            input: json!({"notebook_id": NOTEBOOK_ID, "with": OTHER}),
            caller: Some(OWNER),
        },
    ];
    let (ess_answers, ess_rows) = interpreted(&ir, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "created",
            "not-yours",
            "with-self",
            "not-yours",
            "edited",
            "shared"
        ],
        "the steps reach every branch the caller decides"
    );
    let (answers, rows) = spindled(&ir, &steps);
    assert_eq!(answers, ess_answers, "outcomes, errors and events agree");
    assert_eq!(
        rows, ess_rows,
        "the stored notebook agrees with ESS's view row"
    );
}
