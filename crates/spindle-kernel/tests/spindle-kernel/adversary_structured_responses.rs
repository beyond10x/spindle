//! Adversary cases for `story:structured-response-values`: nested records, unions and maps, a
//! union tagged `value`, constrained newtypes the story's own fixture does not reach, derivation
//! against the kernel's refusals, and a `{generated: true}` record on an outcome only some inputs
//! select.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, OutcomeRef};
use ess_conformance::direct_response::Observation;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value};
use spindle_capability::Capability;
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, execute};

const SYSTEM: &str = r"format: ess/34
system: library
version: v1
summary: A desk that answers notebooks of many shapes.
domains:
  - library.desk
components:
  - component: desk
    summary: Answers questions about desks for callers outside the process.
    owns:
      domains:
        - library.desk
    accepts:
      commands:
        - library.desk.OpenSlot
        - library.desk.ReadDesk
        - library.desk.PriceDesk
        - library.desk.LabelDesk
        - library.desk.CodeDesk
        - library.desk.TrackDesk
        - library.desk.SortDesk
        - library.desk.ChainDesk
    publishes:
      events:
        - library.desk.DeskTracked
        - library.desk.DeskNoted
    reached_by: network
";

const DOMAIN: &str = r#"domain: library.desk
summary: Desks, answered as nested records, unions and maps.
types:
  - name: library.desk.Order
    kind: enum
    variants: [by_title, by_author]
  - name: library.desk.Chain
    kind: struct
    fields:
      - name: pages
        type: Integer
      - name: next
        type: Optional<library.desk.Chain>
  - name: library.desk.Sheet
    kind: struct
    fields:
      - name: pages
        type: Integer
  - name: library.desk.Slot
    kind: union
    tag: value
    variants:
      bound: library.desk.Sheet
      empty: null
  - name: library.desk.Note
    kind: struct
    fields:
      - name: at
        type: Optional<Timestamp>
      - name: tags
        type: Optional<List<String>>
      - name: read
        type: Optional<Boolean>
      - name: raw
        type: Bytes
  - name: library.desk.Drawer
    kind: union
    tag: kind
    variants:
      open: Map<String, library.desk.Note>
      shut: null
  - name: library.desk.Desk
    kind: struct
    fields:
      - name: drawer
        type: library.desk.Drawer
      - name: slot
        type: Optional<library.desk.Slot>
  - name: library.desk.Price
    kind: newtype
    of: Decimal
    invariants:
      - value > 0
  - name: library.desk.Label
    kind: newtype
    of: library.desk.Shelfmark
    invariants:
      - value.count <= 6
  - name: library.desk.Shelfmark
    kind: newtype
    of: String
    invariants:
      - value == "poetry"
  - name: library.desk.Short
    kind: newtype
    of: String
    invariants:
      - value.count <= 3
  - name: library.desk.Coded
    kind: struct
    fields:
      - name: id
        type: Uuid
      - name: code
        type: library.desk.Short
  - name: library.desk.Progress
    kind: struct
    fields:
      - name: done
        type: Optional<Integer>
      - name: at
        type: Timestamp
commands:
  - name: library.desk.SortDesk
    input:
      - name: desk
        type: String
    response:
      - name: order
        type: library.desk.Order
    outcomes:
      - name: sorted
        returns: true
        summary: The order is answered.
  - name: library.desk.ChainDesk
    input:
      - name: desk
        type: String
    response:
      - name: chain
        type: library.desk.Chain
    outcomes:
      - name: chained
        returns: true
        summary: The chain is answered.
  - name: library.desk.OpenSlot
    input:
      - name: desk
        type: String
    response:
      - name: slot
        type: library.desk.Slot
    outcomes:
      - name: opened
        returns: true
        summary: The slot is answered.
  - name: library.desk.ReadDesk
    input:
      - name: desk
        type: String
    response:
      - name: desk
        type: library.desk.Desk
    outcomes:
      - name: read
        returns: true
        summary: The desk is answered.
  - name: library.desk.PriceDesk
    input:
      - name: desk
        type: String
    response:
      - name: price
        type: library.desk.Price
    outcomes:
      - name: priced
        returns: true
        summary: The price is answered.
  - name: library.desk.LabelDesk
    input:
      - name: desk
        type: String
    response:
      - name: label
        type: library.desk.Label
    outcomes:
      - name: labelled
        returns: true
        summary: The label is answered.
  - name: library.desk.CodeDesk
    input:
      - name: desk
        type: String
    response:
      - name: coded
        type: library.desk.Coded
    outcomes:
      - name: coded
        returns: true
        summary: The code is answered.
  - name: library.desk.TrackDesk
    input:
      - name: desk
        type: String
      - name: loud
        type: Boolean
    outcomes:
      - name: tracked-loudly
        when: input.loud == true
        emits: [library.desk.DeskTracked]
        payload:
          library.desk.DeskTracked:
            desk: input.desk
            progress: {generated: true}
        summary: The progress is published.
      - name: tracked-quietly
        emits: [library.desk.DeskNoted]
        payload:
          library.desk.DeskNoted:
            desk: input.desk
        summary: Only the desk is published.
events:
  - name: library.desk.DeskTracked
    fields:
      - name: desk
        type: String
      - name: progress
        type: library.desk.Progress
  - name: library.desk.DeskNoted
    fields:
      - name: desk
        type: String
"#;

fn ir(dir: &Path) -> spindle_spec::EssIr {
    std::fs::create_dir_all(dir.join("domains")).expect("the domains directory");
    std::fs::write(dir.join("system.yaml"), SYSTEM).expect("the system is written");
    std::fs::write(dir.join("domains/desk.yaml"), DOMAIN).expect("the domain is written");
    spindle_spec::load(dir)
        .expect("the desk fixture compiles")
        .ir
}

fn with_ir<T>(run: impl FnOnce(&spindle_spec::EssIr) -> T) -> T {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    run(&ir(dir.path()))
}

fn input() -> Map<String, Value> {
    let mut input = Map::new();
    input.insert("desk".to_owned(), Value::String("oak".to_owned()));
    input
}

fn node_map(response: &Map<String, Value>) -> BTreeMap<String, Node> {
    response
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                serde_json::from_value(value.clone()).expect("a JSON value is a node"),
            )
        })
        .collect()
}

fn spec<'ir>(
    ir: &'ir spindle_spec::EssIr,
    command: &str,
) -> &'ir ess_compiler::ir::ResolvedCommand {
    ir.commands()
        .values()
        .find(|spec| spec.name.to_string() == command)
        .expect("the fixture declares the command")
}

/// ESS's interpreter's answer to `command`, and ESS's response check for its first outcome.
fn interpreted(ir: &spindle_spec::EssIr, command: &str) -> (BTreeMap<String, Node>, Observation) {
    let spec = spec(ir, command);
    let selected = OutcomeRef::new(
        CommandRef::new(spec.name.clone()),
        spec.outcomes[0].name.clone(),
    );
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "library.desk/authored/adversary"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let answered = target
        .execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: node_map(&input()),
            correlation: context.correlation.clone(),
        })
        .expect("the interpreter runs the command");
    let theirs = answered
        .response
        .clone()
        .expect("the interpreter answers a response");
    let check = Observation::of(ir, spec, Some(selected), BTreeMap::new())
        .expect("ESS derives the response check from the command");
    check
        .compare(Some(&theirs))
        .expect("the check accepts the interpreter's own response");
    (theirs, check)
}

/// The kernel's answer to `command`, checked with ESS's response check.
fn answered(ir: &spindle_spec::EssIr, command: &str) -> Map<String, Value> {
    let (_, check) = interpreted(ir, command);
    let response = execute(
        ir,
        &mut MemoryStore::default(),
        &mut NoExternals,
        command,
        &input(),
    )
    .unwrap_or_else(|error| panic!("`{command}` runs: {error}"))
    .response
    .expect("a returning outcome answers a response");
    check
        .compare(Some(&node_map(&response)))
        .unwrap_or_else(|refused| {
            panic!("ESS's response check refuses the kernel's response {response:?}: {refused}")
        });
    response
}

fn reasons(ir: &spindle_spec::EssIr, capability: &'static str) -> Vec<(String, String)> {
    spindle_capability::derive(ir, Some("desk"))
        .expect("desk is a component")
        .get(&Capability::new(capability))
        .map(|reasons| {
            reasons
                .iter()
                .map(|reason| (reason.name.clone(), reason.because.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn adversary_a_union_tagged_value_answers_its_content_under_content() {
    with_ir(|ir| {
        let response = answered(ir, "library.desk.OpenSlot");
        let slot = &response["slot"];
        assert_eq!(
            slot["value"], "bound",
            "the first variant by tag value: {slot}"
        );
        let Value::Object(content) = &slot["content"] else {
            panic!("a tag named `value` carries its content under `content`: {slot}");
        };
        assert!(content["pages"].is_u64(), "{slot}");
        assert_eq!(slot.as_object().map(Map::len), Some(2), "{slot}");
    });
}

#[test]
fn adversary_a_record_of_a_union_of_a_map_of_records_answers_and_needs_the_clock() {
    with_ir(|ir| {
        let response = answered(ir, "library.desk.ReadDesk");
        let drawer = &response["desk"]["drawer"];
        assert_eq!(drawer["kind"], "open", "{response:?}");
        let Value::Object(notes) = &drawer["value"] else {
            panic!("the open drawer holds a map: {drawer}");
        };
        assert_eq!(notes.len(), 1, "{notes:?}");
        let (key, note) = notes.iter().next().expect("one entry");
        assert!(!key.is_empty(), "a text key: {key}");
        assert!(note["at"].is_string(), "{note}");
        assert!(note["raw"].is_string(), "{note}");

        let clock = reasons(ir, Capability::CLOCK);
        assert!(
            clock
                .iter()
                .any(|(name, because)| name == "library.desk.ReadDesk/read"
                    && because.contains("`library.desk.ReadDesk.desk`")),
            "the timestamp inside the map needs the clock: {clock:?}"
        );
    });
}

#[test]
fn adversary_a_decimal_newtype_literal_answers_a_value_of_its_type() {
    with_ir(|ir| {
        let spec = spec(ir, "library.desk.PriceDesk");
        spindle_kernel::runs(ir, spec).expect("a Decimal newtype with a literal runs");
        let response = execute(
            ir,
            &mut MemoryStore::default(),
            &mut NoExternals,
            "library.desk.PriceDesk",
            &input(),
        )
        .expect("pricing runs")
        .response
        .expect("a response");
        spindle_kernel::admits(ir, &spec.response[0].type_ref, &response["price"])
            .unwrap_or_else(|error| panic!("the price is a Price: {error}"));
    });
}

#[test]
fn adversary_a_newtype_over_a_constrained_newtype_answers_the_wrapped_literal_as_ess_does() {
    with_ir(|ir| {
        let (theirs, _) = interpreted(ir, "library.desk.LabelDesk");
        assert_eq!(theirs["label"], Node::Text("poetry".to_owned()));
        let spec = spec(ir, "library.desk.LabelDesk");
        spindle_kernel::runs(ir, spec).unwrap_or_else(|error| {
            panic!(
                "ESS's interpreter answers `poetry` (`base` falls through to the wrapped type's \
                 literal), so the kernel should too: {error}"
            )
        });
        let response = answered(ir, "library.desk.LabelDesk");
        assert_eq!(response["label"], "poetry");
    });
}

#[test]
fn adversary_derivation_names_no_mint_for_a_response_the_kernel_refuses() {
    with_ir(|ir| {
        let spec = spec(ir, "library.desk.CodeDesk");
        let Err(ExecuteError::Unsupported { construct }) = spindle_kernel::runs(ir, spec) else {
            panic!("a record holding a newtype no literal satisfies is refused before it runs");
        };
        assert!(construct.contains("`coded`"), "{construct}");
        let minted = reasons(ir, Capability::IDENTITY_MINT);
        let named: Vec<_> = minted
            .iter()
            .filter(|(name, _)| name.starts_with("library.desk.CodeDesk"))
            .collect();
        assert!(
            named.is_empty(),
            "derivation names identity.mint for a response the kernel refuses by name and never \
             mints: {named:?}"
        );
    });
}

#[test]
fn adversary_a_generated_record_on_an_outcome_some_inputs_select_is_minted_and_derived_there_only()
{
    with_ir(|ir| {
        let spec = spec(ir, "library.desk.TrackDesk");
        spindle_kernel::runs(ir, spec).expect("tracking runs");
        let mut loud = input();
        loud.insert("loud".to_owned(), Value::Bool(true));
        let tracked = execute(
            ir,
            &mut MemoryStore::default(),
            &mut NoExternals,
            "library.desk.TrackDesk",
            &loud,
        )
        .expect("loud tracking runs");
        assert_eq!(tracked.outcome, "tracked-loudly");
        let [event] = tracked.events.as_slice() else {
            panic!("one event: {tracked:?}");
        };
        let progress = &event.payload["progress"];
        assert!(progress["at"].is_string(), "{progress}");
        let event_type = &ir
            .events()
            .values()
            .find(|event| event.name.to_string() == "library.desk.DeskTracked")
            .expect("the event")
            .fields
            .iter()
            .find(|field| field.name == "progress")
            .expect("the progress field")
            .type_ref;
        spindle_kernel::admits(ir, event_type, progress)
            .unwrap_or_else(|error| panic!("the progress is a Progress: {error}"));

        let mut quiet = input();
        quiet.insert("loud".to_owned(), Value::Bool(false));
        let noted = execute(
            ir,
            &mut MemoryStore::default(),
            &mut NoExternals,
            "library.desk.TrackDesk",
            &quiet,
        )
        .expect("quiet tracking runs");
        assert_eq!(noted.outcome, "tracked-quietly");

        let clock = reasons(ir, Capability::CLOCK);
        assert!(
            clock.iter().any(
                |(name, because)| name == "library.desk.TrackDesk/tracked-loudly"
                    && because.contains("`library.desk.DeskTracked.progress`")
            ),
            "{clock:?}"
        );
        let minted = reasons(ir, Capability::IDENTITY_MINT);
        assert!(
            minted.iter().any(
                |(name, because)| name == "library.desk.TrackDesk/tracked-loudly"
                    && because.contains("`library.desk.DeskTracked.progress`")
            ),
            "{minted:?}"
        );
        assert!(
            !clock
                .iter()
                .chain(&minted)
                .any(|(name, _)| name == "library.desk.TrackDesk/tracked-quietly"),
            "the quiet outcome mints nothing: {clock:?} {minted:?}"
        );
    });
}

#[test]
fn adversary_a_recursive_record_terminates_and_is_refused_by_name() {
    with_ir(|ir| {
        let spec = spec(ir, "library.desk.ChainDesk");
        let Err(ExecuteError::Unsupported { construct }) = spindle_kernel::runs(ir, spec) else {
            panic!("a record that holds itself through an Optional is refused by name");
        };
        assert!(construct.contains("`chain`"), "{construct}");
        let _ = reasons(ir, Capability::IDENTITY_MINT);
    });
}

#[test]
fn adversary_ess_interpreter_answers_an_enum_response_field() {
    with_ir(|ir| {
        let (theirs, _) = interpreted(ir, "library.desk.SortDesk");
        assert!(
            matches!(&theirs["order"], Node::Text(text) if text == "by_title" || text == "by_author"),
            "ESS's interpreter chooses an enum response through its input witness: {theirs:?}"
        );
    });
}
