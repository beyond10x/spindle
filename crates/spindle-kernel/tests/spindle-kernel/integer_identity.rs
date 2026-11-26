//! A command names an `Integer` identity whatever its spelling
//! (`story:integer-identity-any-spelling`): ESS keys an instance by the value it reads and compares
//! numbers by their exact value, so `1` and `1.0` name one instance. The kernel writes every
//! `Integer` of a command's input as the integer once ESS admitted it, so the row, its identity and
//! the event payload hold `1`, and so does a caller's `Integer` attribute a command writes. ESS
//! admits no `Integer` past 64 bits, and neither does the kernel.
//!
//! The model is written per test into a scratch directory under the cargo target directory.

use std::collections::BTreeMap;
use std::time::SystemTime;

use ess_compiler::refs::{ActorRef, CommandRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Invocation, MemoryStore, NoExternals, Store, UuidV4, execute, execute_as,
};

use super::related_guards::object;

const SYSTEM: &str = "format: ess/35
system: probe
version: v1
summary: Shelves named by an Integer identity.
domains:
  - probe.shelf
components:
  - component: store
    summary: Opens and touches shelves.
    owns:
      domains:
        - probe.shelf
    accepts:
      commands:
        - probe.shelf.OpenShelf
        - probe.shelf.TouchShelf
        - probe.shelf.ClaimShelf
    publishes:
      events:
        - probe.shelf.ShelfOpened
        - probe.shelf.ShelfTouched
        - probe.shelf.ShelfClaimed
    reached_by: network
";

const DOMAIN: &str = "domain: probe.shelf
summary: Shelves, each named by an Integer identity and holding Integer fields.
types: []
errors: []
actors:
  - name: probe.shelf.Keeper
    attributes:
      - name: keeper
        type: Integer
    may:
      - probe.shelf.OpenShelf
      - probe.shelf.TouchShelf
      - probe.shelf.ClaimShelf
entities:
  - name: probe.shelf.Shelf
    identity:
      name: shelf_id
      type: Integer
    fields:
      - name: count
        type: Integer
      - name: sizes
        type: List<Integer>
      - name: spare
        type: Optional<Integer>
      - name: note
        type: Optional<String>
      - name: keeper
        type: Optional<Integer>
    lifecycle:
      initial: Open
      states: [Open]
      terminal: [Open]
events:
  - name: probe.shelf.ShelfOpened
    fields:
      - name: shelf_id
        type: Integer
      - name: count
        type: Integer
      - name: sizes
        type: List<Integer>
      - name: spare
        type: Optional<Integer>
  - name: probe.shelf.ShelfTouched
    fields:
      - name: shelf_id
        type: Integer
  - name: probe.shelf.ShelfClaimed
    fields:
      - name: shelf_id
        type: Integer
      - name: keeper
        type: Integer
commands:
  - name: probe.shelf.OpenShelf
    input:
      - name: shelf_id
        type: Integer
      - name: count
        type: Integer
      - name: sizes
        type: List<Integer>
      - name: spare
        type: Optional<Integer>
    outcomes:
      - name: opened
        creates: probe.shelf.Shelf
        instance: shelf_id
        sets:
          count: input.count
          sizes: input.sizes
          spare: input.spare
        emits: [probe.shelf.ShelfOpened]
        payload:
          probe.shelf.ShelfOpened:
            shelf_id: input.shelf_id
            count: input.count
            sizes: input.sizes
            spare: input.spare
        summary: The shelf is opened.
  - name: probe.shelf.TouchShelf
    input:
      - name: shelf_id
        type: Integer
      - name: note
        type: String
    outcomes:
      - name: touched
        updates: probe.shelf.Shelf
        instance: shelf_id
        sets:
          note: input.note
        emits: [probe.shelf.ShelfTouched]
        payload:
          probe.shelf.ShelfTouched:
            shelf_id: input.shelf_id
        summary: The shelf is touched.
  - name: probe.shelf.ClaimShelf
    input:
      - name: shelf_id
        type: Integer
    outcomes:
      - name: claimed
        updates: probe.shelf.Shelf
        instance: shelf_id
        sets:
          keeper: {caller: keeper}
        emits: [probe.shelf.ShelfClaimed]
        payload:
          probe.shelf.ShelfClaimed:
            shelf_id: input.shelf_id
            keeper: {caller: keeper}
        summary: The shelf is claimed by the keeper sending the command.
";

const SHELF: &str = "probe.shelf.Shelf";
const OPEN: &str = "probe.shelf.OpenShelf";
const TOUCH: &str = "probe.shelf.TouchShelf";
const CLAIM: &str = "probe.shelf.ClaimShelf";
const KEEPER: &str = "probe.shelf.Keeper";

/// The model, in a directory of its own.
fn ir() -> (tempfile::TempDir, spindle_spec::EssIr) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    std::fs::write(dir.path().join("system.yaml"), SYSTEM).expect("the system writes");
    std::fs::write(dir.path().join("domains/shelf.yaml"), DOMAIN).expect("the domain writes");
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the shelf model compiles: {error}"))
        .ir;
    (dir, ir)
}

/// `input` parsed from its JSON text, so a number keeps the spelling it is written with.
fn input(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON input")
}

/// An `OpenShelf` input for `shelf_id`, spelled as given.
fn open(shelf_id: &str) -> Value {
    input(&format!(
        r#"{{"shelf_id": {shelf_id}, "count": 2, "sizes": [3], "spare": 4}}"#
    ))
}

/// A `TouchShelf` input for `shelf_id`, spelled as given.
fn touch(shelf_id: &str) -> Value {
    input(&format!(r#"{{"shelf_id": {shelf_id}, "note": "seen"}}"#))
}

/// The kernel's answer to each step on one memory store: the outcome, or its refusal.
fn spindled(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    steps: &[(&str, Value)],
) -> Vec<Result<String, ExecuteError>> {
    steps
        .iter()
        .map(|(command, input)| {
            execute(ir, store, &mut NoExternals, command, &object(input.clone()))
                .map(|executed| executed.outcome)
        })
        .collect()
}

/// ESS 0.99.1's interpreter's answer to each step, in one scenario: the outcome, or its refusal.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<Result<String, String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.shelf/authored/integer-identity"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("integer-identity").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    steps
        .iter()
        .map(|(command, input)| {
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: object(input.clone())
                        .iter()
                        .map(|(field, value)| {
                            (
                                field.clone(),
                                serde_json::from_value(value.clone()).expect("a node"),
                            )
                        })
                        .collect(),
                    correlation: context.correlation.clone(),
                })
                .map(|executed| {
                    executed
                        .outcome
                        .map(|outcome| outcome.outcome.as_str().to_owned())
                        .unwrap_or_default()
                })
                .map_err(|error| error.to_string())
        })
        .collect()
}

/// The kernel's outcome for each of `steps`, each the interpreter's (`None` where both refuse),
/// and the store they ran on.
fn agreed(steps: &[(&str, Value)]) -> (Vec<Option<String>>, MemoryStore) {
    let (_dir, ir) = ir();
    let mut store = MemoryStore::default();
    let ours = spindled(&ir, &mut store, steps);
    let theirs = interpreted(&ir, steps);
    let outcomes = ours
        .into_iter()
        .zip(theirs)
        .enumerate()
        .map(|(step, (ours, theirs))| match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => {
                assert_eq!(ours, theirs, "step {step}: {}", steps[step].1);
                Some(ours)
            }
            (Err(_), Err(_)) => None,
            (ours, theirs) => panic!(
                "step {step} ({}): the kernel answers {ours:?}, the interpreter {theirs:?}",
                steps[step].1
            ),
        })
        .collect();
    (outcomes, store)
}

/// Every shelf the store holds, by the identity it is keyed by.
fn shelves(store: &mut MemoryStore) -> Vec<Value> {
    store
        .scan(SHELF)
        .expect("the store scans")
        .rows
        .into_iter()
        .map(|(identity, _)| identity)
        .collect()
}

#[test]
fn a_command_names_an_integer_identity_whatever_its_spelling() {
    for (created, named) in [("1.0", "1"), ("1", "1.0")] {
        let (outcomes, mut store) = agreed(&[(OPEN, open(created)), (TOUCH, touch(named))]);
        assert_eq!(
            outcomes,
            [Some("opened".to_owned()), Some("touched".to_owned())],
            "a shelf created as {created} is found as {named}"
        );
        assert_eq!(
            shelves(&mut store),
            [json!(1)],
            "one shelf, keyed as the integer"
        );
        let (outcomes, mut store) = agreed(&[(OPEN, open(created)), (OPEN, open(named))]);
        assert_eq!(
            outcomes,
            [Some("opened".to_owned()), None],
            "a second creation as {named} meets the shelf created as {created}, as ESS does"
        );
        assert_eq!(shelves(&mut store), [json!(1)], "still one shelf");
    }
}

#[test]
fn a_created_row_answers_its_integer_identity_and_fields_as_integers() {
    let (_dir, ir) = ir();
    let mut store = MemoryStore::default();
    let executed = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        OPEN,
        &object(input(
            r#"{"shelf_id": 1.0, "count": 2.0, "sizes": [3.0], "spare": 4.0}"#,
        )),
    )
    .expect("the shelf opens");
    assert_eq!(executed.outcome, "opened");
    assert_eq!(
        Value::Object(executed.events[0].payload.clone()),
        json!({"shelf_id": 1, "count": 2, "sizes": [3], "spare": 4}),
        "the event payload answers integers"
    );
    assert_eq!(
        shelves(&mut store),
        [json!(1)],
        "the identity is the integer"
    );
    let held = store
        .load(SHELF, &json!(1))
        .expect("the store reads")
        .expect("the shelf is held under 1");
    assert_eq!(
        serde_json::to_value(&held.row.fields).expect("fields serialize"),
        json!({"count": 2, "sizes": [3], "spare": 4}),
        "the row answers integers"
    );
}

/// ESS 0.99.1 admits an `Integer` only within `i64` however it is spelt (`Number::is_integral` is
/// `as_i64`), so an integral value past 64 bits never reaches a row: the kernel refuses it as ESS
/// does, whatever its spelling, and nothing is held. The bounds themselves run and are found by
/// either spelling.
#[test]
fn an_integer_identity_past_64_bits_is_held_as_written() {
    for wide in ["9223372036854775808", "18446744073709551616", "1e20"] {
        let (outcomes, mut store) = agreed(&[(OPEN, open(wide)), (TOUCH, touch(wide))]);
        assert_eq!(
            outcomes,
            [None, None],
            "{wide} is refused by both, as ESS refuses it"
        );
        assert!(shelves(&mut store).is_empty(), "nothing is held for {wide}");
    }
    for (created, named, held) in [
        (
            "9223372036854775807",
            "9223372036854775807",
            json!(i64::MAX),
        ),
        (
            "-9223372036854775808.0",
            "-9223372036854775808",
            json!(i64::MIN),
        ),
    ] {
        let (outcomes, mut store) = agreed(&[(OPEN, open(created)), (TOUCH, touch(named))]);
        assert_eq!(
            outcomes,
            [Some("opened".to_owned()), Some("touched".to_owned())],
            "a shelf created as {created} is found as {named}"
        );
        assert_eq!(
            shelves(&mut store),
            [held],
            "the bound is held as the integer"
        );
    }
}

/// Shelf 1 opened, then claimed by a keeper whose `keeper` attribute is spelled `keeper`: the
/// kernel's answer to the claim (the event payload, or the refusal) and the shelf's `keeper`
/// field after it, each checked against ESS's interpreter (the same outcome, or a refusal both).
fn claimed(keeper: &str) -> (Result<Value, ExecuteError>, Option<Value>) {
    let (_dir, ir) = ir();
    let caller = object(input(&format!(r#"{{"keeper": {keeper}}}"#)));
    let mut store = MemoryStore::default();
    execute(&ir, &mut store, &mut NoExternals, OPEN, &object(open("1"))).expect("the shelf opens");
    let ours = execute_as(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &SystemTime::now,
        &Invocation {
            command: CLAIM,
            input: &object(touch_free("1")),
            actor: Some(KEEPER),
            caller: Some(&caller),
        },
    );

    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.shelf/authored/integer-caller"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("integer-caller").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let nodes = |value: Value| -> BTreeMap<String, Node> {
        object(value)
            .into_iter()
            .map(|(field, value)| (field, serde_json::from_value(value).expect("a node")))
            .collect()
    };
    let request = |command: &str, input: Value, caller: Option<&Map<String, Value>>| {
        target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: caller.map(|_| ActorRef::new(KEEPER.parse().expect("an actor name"))),
                caller: caller.map(|caller| nodes(Value::Object(caller.clone()))),
                input: nodes(input),
                correlation: context.correlation.clone(),
            })
            .map(|executed| {
                executed
                    .outcome
                    .map(|outcome| outcome.outcome.as_str().to_owned())
                    .unwrap_or_default()
            })
            .map_err(|error| error.to_string())
    };
    assert_eq!(
        request(OPEN, open("1"), None),
        Ok("opened".to_owned()),
        "the interpreter opens the shelf"
    );
    let theirs = request(CLAIM, touch_free("1"), Some(&caller));
    match (&ours, &theirs) {
        (Ok(ours), Ok(theirs)) => assert_eq!(&ours.outcome, theirs, "the claim as {keeper}"),
        (Err(_), Err(_)) => {}
        _ => {
            panic!("the claim as {keeper}: the kernel answers {ours:?}, the interpreter {theirs:?}")
        }
    }
    let held = store
        .load(SHELF, &json!(1))
        .expect("the store reads")
        .expect("shelf 1 is held")
        .row
        .fields
        .get("keeper")
        .cloned();
    (
        ours.map(|executed| Value::Object(executed.events[0].payload.clone())),
        held,
    )
}

/// The input of a command naming shelf `shelf_id` alone.
fn touch_free(shelf_id: &str) -> Value {
    input(&format!(r#"{{"shelf_id": {shelf_id}}}"#))
}

#[test]
fn a_caller_integer_attribute_is_held_as_the_integer() {
    for keeper in ["7.0", "7"] {
        let (payload, held) = claimed(keeper);
        assert_eq!(
            payload.expect("the claim runs"),
            json!({"shelf_id": 1, "keeper": 7}),
            "the event publishes the caller's {keeper} as the integer"
        );
        assert_eq!(
            held,
            Some(json!(7)),
            "the shelf holds the caller's {keeper} as the integer"
        );
    }
    for wide in ["9223372036854775808", "18446744073709551616", "1e20"] {
        let (payload, held) = claimed(wide);
        assert!(
            matches!(payload, Err(ExecuteError::InvalidInput(_))),
            "a caller's {wide} is refused as invalid input, as on the input path: {payload:?}"
        );
        assert_eq!(held, None, "nothing is written for the caller's {wide}");
    }
}
