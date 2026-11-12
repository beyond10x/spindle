//! A payload value read from the command's own response (`{response: <field>}`, ESS
//! `ResolvedPayloadValue::ResponseField`), as ESS's interpreter reads it
//! (`interpret::execute::determined::response`, `interpret::execute::value_at`): the value this
//! invocation answers in that field, prepared before a creation takes its identity and before the
//! events are emitted (`interpret::execute::take`).
//!
//! The fixture `tests/fixtures/response-source-reads`: `RegisterParcel` creates a parcel under
//! the `id` its answer returns and publishes the answer's `Optional` `receipt`; `RelabelParcel`
//! updates one and publishes the `ticket` its answer returns, at the top level and in a struct;
//! `CopyParcel` stores, through `{payload: ParcelCopied.receipt}`, a value its event reads from
//! the answer, which ESS's interpreter does not read (it resolves stored payload reads before the
//! response is prepared) and the kernel refuses.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, OutcomeRef};
use ess_conformance::direct_response::Observation;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, ScenarioContext, SemanticCommandRequest, SemanticCommandResult,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{ExecuteError, Executed, MemoryStore, NoExternals, Store, execute};

const REGISTER: &str = "parcels.desk.RegisterParcel";
const RELABEL: &str = "parcels.desk.RelabelParcel";
const COPY: &str = "parcels.desk.CopyParcel";
const PARCEL: &str = "parcels.desk.Parcel";

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/response-source-reads");
    spindle_spec::load(&root)
        .expect("the response-source-reads fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

fn response(executed: &Executed) -> &Map<String, Value> {
    executed
        .response
        .as_ref()
        .unwrap_or_else(|| panic!("`{}` answers a response: {executed:?}", executed.outcome))
}

fn payload<'e>(executed: &'e Executed, event: &str) -> &'e Map<String, Value> {
    &executed
        .events
        .iter()
        .find(|published| published.name == event)
        .unwrap_or_else(|| panic!("`{}` publishes {event}: {executed:?}", executed.outcome))
        .payload
}

fn node_map(values: &Map<String, Value>) -> BTreeMap<String, Node> {
    values
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                serde_json::from_value(value.clone()).expect("a JSON value is a node"),
            )
        })
        .collect()
}

/// Fails unless ESS's response check, derived for the outcome the kernel took, accepts its answer.
fn checks(ir: &spindle_spec::EssIr, command: &str, executed: &Executed) {
    let declared = ir
        .commands()
        .values()
        .find(|declared| declared.name.to_string() == command)
        .expect("the fixture declares the command");
    let taken = declared
        .outcomes
        .iter()
        .find(|outcome| outcome.name.as_str() == executed.outcome)
        .expect("the command declares the outcome taken");
    let selected = OutcomeRef::new(CommandRef::new(declared.name.clone()), taken.name.clone());
    let check = Observation::of(ir, declared, Some(selected), BTreeMap::new())
        .expect("ESS derives the response check from the command");
    check
        .compare(Some(&node_map(response(executed))))
        .unwrap_or_else(|refused| {
            panic!("ESS's response check refuses the kernel's answer {executed:?}: {refused}")
        });
}

#[test]
fn a_creation_publishes_and_is_created_under_the_answered_id() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut answered = Vec::new();
    for label in ["fragile", "heavy"] {
        let executed = execute(
            &ir,
            &mut store,
            &mut NoExternals,
            REGISTER,
            &input(json!({"label": label})),
        )
        .expect("registering runs");
        assert_eq!(executed.outcome, "registered");
        let answer = response(&executed);
        let published = payload(&executed, "parcels.desk.ParcelRegistered");
        assert_eq!(
            published.get("id"),
            answer.get("id"),
            "the event publishes the id the answer returns: {executed:?}"
        );
        assert_eq!(
            published.get("receipt"),
            answer.get("receipt"),
            "an `Optional` response field the answer holds is published as answered: {executed:?}"
        );
        assert!(answer.get("receipt").is_some_and(Value::is_string));
        checks(&ir, REGISTER, &executed);
        let id = answer["id"].clone();
        let row = store
            .load(PARCEL, &id)
            .expect("the store reads")
            .unwrap_or_else(|| panic!("a parcel rests under the answered id {id}"));
        assert_eq!(row.row.fields.get("label"), Some(&json!(label)));
        answered.push(id);
    }
    assert_ne!(answered[0], answered[1], "each request answers its own id");
}

#[test]
fn an_update_publishes_the_ticket_it_answers() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let registered = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        REGISTER,
        &input(json!({"label": "fragile"})),
    )
    .expect("registering runs");
    let id = response(&registered)["id"].clone();
    let relabelled = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        RELABEL,
        &input(json!({"id": id, "label": "heavy"})),
    )
    .expect("relabelling runs");
    assert_eq!(relabelled.outcome, "relabelled");
    let ticket = &response(&relabelled)["ticket"];
    let published = payload(&relabelled, "parcels.desk.ParcelRelabelled");
    assert_eq!(published.get("ticket"), Some(ticket), "{relabelled:?}");
    assert_eq!(
        published.get("stamp"),
        Some(&json!({"ticket": ticket, "label": "heavy"})),
        "a struct member reads the same answer: {relabelled:?}"
    );
    checks(&ir, RELABEL, &relabelled);
}

#[test]
fn a_stored_copy_of_an_answered_value_is_refused_before_anything_is_written() {
    let ir = ir();
    let command = ir
        .commands()
        .values()
        .find(|declared| declared.name.to_string() == COPY)
        .expect("the fixture declares CopyParcel");
    let refused = spindle_kernel::runs(&ir, command).expect_err("the copy does not run");
    let ExecuteError::Unsupported { construct } = &refused else {
        panic!("the copy is refused as a construct the kernel does not run: {refused:?}")
    };
    assert!(
        construct.contains("parcels.desk.ParcelCopied.receipt")
            && construct.contains("response field `receipt`"),
        "the refusal names the stored read and the response field it reaches: {construct}"
    );
    let mut store = MemoryStore::default();
    let executed = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        COPY,
        &input(json!({"label": "fragile"})),
    );
    assert!(
        matches!(executed, Err(ExecuteError::Unsupported { .. })),
        "{executed:?}"
    );
    assert_eq!(
        store
            .events_after(0, usize::MAX)
            .expect("the store reads")
            .len(),
        0,
        "nothing is written"
    );
}

/// ESS's interpreter, on the same commands: the creation's identity and the published values are
/// the ones it answers, and the stored copy is a construct it does not interpret.
#[test]
fn the_interpreter_reads_the_same_answers() {
    let ir = ir();
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "parcels.desk/authored/response-source"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("response-source").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let run = |command: &str, input: &Map<String, Value>| {
        target.execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: node_map(input),
            correlation: context.correlation.clone(),
        })
    };
    let event = |result: &SemanticCommandResult, field: &str| {
        result
            .direct_events
            .first()
            .and_then(|event| event.payload.get(field))
            .cloned()
    };
    let registered =
        run(REGISTER, &input(json!({"label": "fragile"}))).expect("the interpreter registers");
    let answer = registered
        .response
        .clone()
        .expect("the interpreter answers a response");
    assert_eq!(event(&registered, "id"), answer.get("id").cloned());
    assert_eq!(
        event(&registered, "receipt"),
        answer.get("receipt").cloned()
    );
    let id: Value = serde_json::to_value(&answer["id"]).expect("a node is JSON");
    let relabelled = run(RELABEL, &input(json!({"id": id, "label": "heavy"})))
        .expect("the interpreter relabels the parcel created under the answered id");
    let ticket = relabelled
        .response
        .as_ref()
        .and_then(|answer| answer.get("ticket"))
        .cloned();
    assert!(ticket.is_some());
    assert_eq!(event(&relabelled, "ticket"), ticket);
    assert!(
        run(COPY, &input(json!({"label": "fragile"}))).is_err(),
        "ESS's interpreter does not interpret a stored copy of an answered value"
    );
}
