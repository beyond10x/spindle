//! A returning outcome answers a response field of a record, union, map, list or constrained
//! newtype with a value of that type, as ESS's interpreter chooses one
//! (`interpret::response::base`): a record's members each by the same rules, a union's first
//! variant, one map entry, a newtype's invariant literal where one satisfies it. An enum field,
//! which `base` does not choose, is refused by name before the command runs. A `{generated: true}`
//! payload field of a record type is minted by the same rules, as ESS mints one
//! (`interpret::values::mint`).

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, OutcomeRef};
use ess_conformance::direct_response::Observation;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value};
use spindle_kernel::{ExecuteError, Executed, MemoryStore, NoExternals, execute};

const SYSTEM: &str = r"format: ess/34
system: library
version: v1
summary: A shelf desk that answers layouts, placements, counts, ratings and orders of books.
domains:
  - library.shelf
components:
  - component: desk
    summary: Answers questions about shelves for callers outside the process.
    owns:
      domains:
        - library.shelf
    accepts:
      commands:
        - library.shelf.ArrangeShelf
        - library.shelf.PlaceBook
        - library.shelf.CountShelf
        - library.shelf.ListShelves
        - library.shelf.RateShelf
        - library.shelf.ClassifyShelf
        - library.shelf.SortShelf
        - library.shelf.TrackShelf
    publishes:
      events:
        - library.shelf.ShelfTracked
    reached_by: network
";

const DOMAIN: &str = r#"domain: library.shelf
summary: Shelves of books, answered as records, unions, maps, lists, ratings and orders.
types:
  - name: library.shelf.Layout
    kind: struct
    fields:
      - name: page_size
        type: Optional<Integer>
      - name: columns
        type: Optional<Integer>
  - name: library.shelf.Borrower
    kind: newtype
    of: String
  - name: library.shelf.Placement
    kind: union
    tag: kind
    variants:
      lent: library.shelf.Borrower
      shelved: library.shelf.Layout
  - name: library.shelf.Rating
    kind: newtype
    of: Integer
    invariants:
      - value >= 1
      - value <= 5
  - name: library.shelf.Genre
    kind: newtype
    of: String
    invariants:
      - value == "poetry"
  - name: library.shelf.Order
    kind: enum
    variants: [by_title, by_author]
commands:
  - name: library.shelf.ArrangeShelf
    input:
      - name: shelf
        type: String
    response:
      - name: item
        type: library.shelf.Layout
    outcomes:
      - name: arranged
        returns: true
        summary: The shelf's layout is answered.
  - name: library.shelf.PlaceBook
    input:
      - name: shelf
        type: String
    response:
      - name: placement
        type: library.shelf.Placement
    outcomes:
      - name: placed
        returns: true
        summary: Where the book is is answered.
  - name: library.shelf.CountShelf
    input:
      - name: shelf
        type: String
    response:
      - name: counts
        type: Map<String, Integer>
    outcomes:
      - name: counted
        returns: true
        summary: The books per author are answered.
  - name: library.shelf.ListShelves
    input:
      - name: shelf
        type: String
    response:
      - name: layouts
        type: List<library.shelf.Layout>
    outcomes:
      - name: listed
        returns: true
        summary: The layouts of the shelves are answered.
  - name: library.shelf.RateShelf
    input:
      - name: shelf
        type: String
    response:
      - name: rating
        type: library.shelf.Rating
    outcomes:
      - name: rated
        returns: true
        summary: The shelf's rating is answered.
  - name: library.shelf.ClassifyShelf
    input:
      - name: shelf
        type: String
    response:
      - name: genre
        type: library.shelf.Genre
    outcomes:
      - name: classified
        returns: true
        summary: The shelf's genre is answered.
  - name: library.shelf.SortShelf
    input:
      - name: shelf
        type: String
    response:
      - name: order
        type: library.shelf.Order
    outcomes:
      - name: sorted
        returns: true
        summary: The shelf's order is answered.
  - name: library.shelf.TrackShelf
    input:
      - name: shelf
        type: String
    outcomes:
      - name: tracked
        emits: [library.shelf.ShelfTracked]
        payload:
          library.shelf.ShelfTracked:
            shelf: input.shelf
            progress: {generated: true}
        summary: The shelf's progress is published as a layout the runtime chooses.
events:
  - name: library.shelf.ShelfTracked
    fields:
      - name: shelf
        type: String
      - name: progress
        type: library.shelf.Layout
"#;

fn ir(dir: &Path) -> spindle_spec::EssIr {
    std::fs::create_dir_all(dir.join("domains")).expect("the domains directory");
    std::fs::write(dir.join("system.yaml"), SYSTEM).expect("the system is written");
    std::fs::write(dir.join("domains/shelf.yaml"), DOMAIN).expect("the domain is written");
    spindle_spec::load(dir)
        .expect("the shelf fixture compiles")
        .ir
}

fn with_ir<T>(run: impl FnOnce(&spindle_spec::EssIr) -> T) -> T {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    run(&ir(dir.path()))
}

fn input() -> Map<String, Value> {
    let mut input = Map::new();
    input.insert("shelf".to_owned(), Value::String("north".to_owned()));
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

/// Runs `command` on the kernel and on ESS's interpreter, each from an empty store; returns the
/// kernel's answer and the interpreter's, and the command and outcome the kernel selected.
fn run_both<'ir>(
    ir: &'ir spindle_spec::EssIr,
    command: &str,
) -> (
    Map<String, Value>,
    BTreeMap<String, Node>,
    &'ir ess_compiler::ir::ResolvedCommand,
    OutcomeRef,
) {
    let executed: Executed = execute(
        ir,
        &mut MemoryStore::default(),
        &mut NoExternals,
        command,
        &input(),
    )
    .unwrap_or_else(|error| panic!("`{command}` runs: {error}"));
    let response = executed
        .response
        .clone()
        .unwrap_or_else(|| panic!("a returning outcome answers a response: {executed:?}"));
    let spec = ir
        .commands()
        .values()
        .find(|spec| spec.name.to_string() == command)
        .expect("the fixture declares the command");
    let selected = OutcomeRef::new(
        CommandRef::new(spec.name.clone()),
        spec.outcomes[0].name.clone(),
    );
    assert_eq!(executed.outcome, selected.outcome.as_str());

    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "library.shelf/authored/structured"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("structured").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let interpreted = target
        .execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: node_map(&input()),
            correlation: context.correlation.clone(),
        })
        .expect("the interpreter runs the command");
    let theirs = interpreted
        .response
        .clone()
        .expect("the interpreter answers a response");
    (response, theirs, spec, selected)
}

/// [`run_both`], the kernel's answer checked with ESS's response check for the same command and
/// outcome, which must also accept the interpreter's own answer.
fn answered(
    ir: &spindle_spec::EssIr,
    command: &str,
) -> (Map<String, Value>, BTreeMap<String, Node>) {
    let (response, theirs, spec, selected) = run_both(ir, command);
    let check = Observation::of(ir, spec, Some(selected), BTreeMap::new())
        .expect("ESS derives the response check from the command");
    check
        .compare(Some(&theirs))
        .expect("the check accepts the interpreter's own response");
    let ours = node_map(&response);
    check.compare(Some(&ours)).unwrap_or_else(|refused| {
        panic!("ESS's response check refuses the kernel's response {response:?}: {refused}")
    });
    (response, theirs)
}

fn assert_layout(value: &Value, at: &str) {
    let Value::Object(members) = value else {
        panic!("{at} is a record, not {value}");
    };
    assert_eq!(
        members.keys().cloned().collect::<Vec<_>>(),
        vec!["columns".to_owned(), "page_size".to_owned()],
        "{at} holds each member, an optional one as its required type: {value}"
    );
    for (name, member) in members {
        assert!(
            member.is_i64() || member.is_u64(),
            "{at}.{name} is an integer: {member}"
        );
    }
}

#[test]
fn a_record_response_field_answers_a_record() {
    with_ir(|ir| {
        let (response, _) = answered(ir, "library.shelf.ArrangeShelf");
        assert_layout(&response["item"], "item");
    });
}

#[test]
fn union_map_and_list_response_fields_answer_their_type() {
    with_ir(|ir| {
        let (response, _) = answered(ir, "library.shelf.PlaceBook");
        let placement = &response["placement"];
        assert_eq!(
            placement["kind"], "lent",
            "a union answers its first variant: {placement}"
        );
        assert!(
            placement["value"].is_string(),
            "the variant's content under `value`: {placement}"
        );

        let (response, _) = answered(ir, "library.shelf.CountShelf");
        let Value::Object(counts) = &response["counts"] else {
            panic!("a map answers an object: {response:?}");
        };
        assert_eq!(counts.len(), 1, "a map answers one entry: {counts:?}");
        assert!(counts.values().all(Value::is_i64), "{counts:?}");

        let (response, _) = answered(ir, "library.shelf.ListShelves");
        let Value::Array(layouts) = &response["layouts"] else {
            panic!("a list answers an array: {response:?}");
        };
        for (index, layout) in layouts.iter().enumerate() {
            assert_layout(layout, &format!("layouts[{index}]"));
        }
    });
}

#[test]
fn a_constrained_newtype_response_field_satisfies_its_invariants() {
    with_ir(|ir| {
        let (response, theirs) = answered(ir, "library.shelf.ClassifyShelf");
        assert_eq!(response["genre"], "poetry");
        assert_eq!(
            node_map(&response)["genre"],
            theirs["genre"],
            "the invariant's literal, as the interpreter chooses it"
        );

        // ESS's response check reads only a String newtype's constraints, so an Integer
        // newtype's answer is checked against its declared type and the interpreter's choice.
        let (response, theirs, spec, _) = run_both(ir, "library.shelf.RateShelf");
        assert_eq!(response["rating"], 1);
        assert_eq!(
            node_map(&response)["rating"],
            theirs["rating"],
            "the first invariant literal that satisfies every invariant, as the interpreter chooses"
        );
        spindle_kernel::admits(ir, &spec.response[0].type_ref, &response["rating"])
            .unwrap_or_else(|error| panic!("the rating is a value of its type: {error}"));
    });
}

#[test]
fn an_enum_response_field_is_refused_by_name_before_the_command_runs() {
    with_ir(|ir| {
        let spec = ir
            .commands()
            .values()
            .find(|spec| spec.name.to_string() == "library.shelf.SortShelf")
            .expect("the fixture declares SortShelf");
        let Err(ExecuteError::Unsupported { construct }) = spindle_kernel::runs(ir, spec) else {
            panic!("an enum response field is refused before the command runs");
        };
        assert!(
            construct.contains("`order`") && construct.contains("library.shelf.Order"),
            "the refusal names the field and its type: {construct}"
        );
        let mut store = MemoryStore::default();
        let refused = execute(
            ir,
            &mut store,
            &mut NoExternals,
            "library.shelf.SortShelf",
            &input(),
        );
        assert!(
            matches!(refused, Err(ExecuteError::Unsupported { .. })),
            "{refused:?}"
        );
    });
}

#[test]
fn a_generated_record_payload_field_is_minted_as_a_record() {
    with_ir(|ir| {
        let tracked = execute(
            ir,
            &mut MemoryStore::default(),
            &mut NoExternals,
            "library.shelf.TrackShelf",
            &input(),
        )
        .expect("tracking runs");
        let [event] = tracked.events.as_slice() else {
            panic!("one event: {tracked:?}");
        };
        let progress = &event.payload["progress"];
        assert_layout(progress, "progress");
        let layout = &ir
            .commands()
            .values()
            .find(|spec| spec.name.to_string() == "library.shelf.ArrangeShelf")
            .expect("the fixture declares ArrangeShelf")
            .response[0]
            .type_ref;
        spindle_kernel::admits(ir, layout, progress)
            .unwrap_or_else(|error| panic!("the progress is a layout: {error}"));
    });
}

#[test]
fn derivation_names_identity_mint_for_each_minted_member() {
    with_ir(|ir| {
        let requirements =
            spindle_capability::derive(ir, Some("desk")).expect("desk is a component");
        let minted = requirements
            .get(&spindle_capability::Capability::new(
                spindle_capability::Capability::IDENTITY_MINT,
            ))
            .expect("the record members are minted through identity.mint");
        let because: Vec<(&str, &str)> = minted
            .iter()
            .map(|reason| (reason.name.as_str(), reason.because.as_str()))
            .collect();
        for (outcome, at) in [
            (
                "library.shelf.ArrangeShelf/arranged",
                "`library.shelf.ArrangeShelf.item`",
            ),
            (
                "library.shelf.PlaceBook/placed",
                "`library.shelf.PlaceBook.placement`",
            ),
            (
                "library.shelf.CountShelf/counted",
                "`library.shelf.CountShelf.counts`",
            ),
            (
                "library.shelf.TrackShelf/tracked",
                "`library.shelf.ShelfTracked.progress`",
            ),
        ] {
            assert!(
                because
                    .iter()
                    .any(|(name, why)| *name == outcome && why.contains(at)),
                "{outcome} requires identity.mint for {at}: {because:?}"
            );
        }
        assert!(
            !because
                .iter()
                .any(|(name, _)| name.starts_with("library.shelf.RateShelf")),
            "a rating chosen from its invariants mints nothing: {because:?}"
        );
    });
}
