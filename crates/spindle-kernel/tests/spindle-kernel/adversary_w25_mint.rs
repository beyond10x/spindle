//! Adversary cases for `story:identity-mint-types-and-fallback`: minted `Integer` identities and
//! value-source fallbacks, compared with ESS's interpreter where it reads the same construct.
//!
//! The fixture `tests/fixtures/adversary_w25_mint` declares a crate under an `Integer` identity
//! the runtime mints, renamed by a later command that takes that identity as input; a tag under
//! an `Integer` newtype with an invariant (`value >= 1`); a slip whose fields fall back to a
//! literal of each literal-spelled type (`String` over a nested input path, enum, `Decimal`,
//! `Boolean`; ESS admits no `Timestamp` or `Uuid` literal) and whose refusal's fields fall back to another input and over
//! an input path; a backed slip whose creation identity falls back to another input; and a
//! grumble whose error field falls back to a literal after a plain input.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Identity, MemoryStore, NoExternals, Store, Uuid, UuidV4, execute, execute_minting,
};

const CRATE: &str = "advw25.box.Crate";
const OPEN_CRATE: &str = "advw25.box.OpenCrate";
const RENAME_CRATE: &str = "advw25.box.RenameCrate";
const OPEN_TAGGED: &str = "advw25.box.OpenTagged";
const FILE_SLIP: &str = "advw25.box.FileSlip";
const FILE_BACKED: &str = "advw25.box.FileBacked";
const GRUMBLE: &str = "advw25.box.Grumble";

const SLIP: &str = "5a5a5a5a-0000-4000-8000-000000000001";
const SLIP_TWO: &str = "5a5a5a5a-0000-4000-8000-000000000002";
const SLIP_THREE: &str = "5a5a5a5a-0000-4000-8000-000000000003";
const SLIP_FOUR: &str = "5a5a5a5a-0000-4000-8000-000000000004";
const BACKED: &str = "6b6b6b6b-0000-4000-8000-000000000001";
const BACKUP: &str = "6b6b6b6b-0000-4000-8000-000000000002";
const OTHER_BACKUP: &str = "6b6b6b6b-0000-4000-8000-000000000003";

/// The largest integer every JSON reader holds exactly: 2^53 - 1.
const JSON_SAFE: u64 = (1 << 53) - 1;

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_w25_mint");
    spindle_spec::load(&root)
        .unwrap_or_else(|error| panic!("the adversary_w25_mint fixture compiles: {error:?}"))
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn spec<'ir>(
    ir: &'ir spindle_spec::EssIr,
    command: &str,
) -> &'ir ess_compiler::ir::ResolvedCommand {
    ir.commands()
        .values()
        .find(|spec| spec.name.to_string() == command)
        .unwrap_or_else(|| panic!("the fixture declares {command}"))
}

// ---- Integer identities --------------------------------------------------------------------

/// A mint that answers the UUIDs given, in order.
fn answering(uuids: Vec<Uuid>) -> impl FnMut(&Identity<'_>) -> Uuid {
    let mut uuids = uuids.into_iter();
    move |identity: &Identity<'_>| {
        uuids
            .next()
            .unwrap_or_else(|| panic!("no UUID left for {}", identity.field))
    }
}

/// The extremes of a UUID's bits give the extremes of a minted `Integer` identity: never
/// negative, never past 2^53 - 1, so a JSON reader that holds numbers as doubles (a browser, the
/// Node SDK) reads it exactly, and the store keys the row under that same number.
#[test]
fn adv_a_minted_integer_identity_stays_within_the_json_safe_range() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut mint = answering(vec![Uuid::max(), Uuid::nil()]);
    let mut minted = Vec::new();
    for name in ["max", "nil"] {
        let opened = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            OPEN_CRATE,
            &object(json!({"name": name})),
        )
        .unwrap_or_else(|error| panic!("{name}: the crate opens: {error}"));
        assert_eq!(opened.outcome, "opened", "{name}");
        let identity = opened.events[0].payload["crate_id"].clone();
        let number = identity
            .as_u64()
            .unwrap_or_else(|| panic!("{name}: a non-negative integer identity, not {identity}"));
        assert!(number <= JSON_SAFE, "{name}: {number} is past 2^53 - 1");
        let stored = store
            .load(CRATE, &identity)
            .expect("the store reads")
            .unwrap_or_else(|| panic!("{name}: stored under {identity}"));
        assert_eq!(stored.row.fields.get("name"), Some(&json!(name)));
        minted.push(number);
    }
    assert_eq!(minted[0], JSON_SAFE, "all ones keeps the low 53 bits");
    assert_eq!(minted[1], 0, "the nil UUID mints zero");
}

/// Two distinct fresh UUIDs whose low 53 bits agree (nil and `1 << 53`) mint one `Integer`
/// identity, and the second creation is refused as a held identity, where ESS's interpreter draws
/// again on a held identity (`interpret::execute::creation_identity`) and the `Mint` contract
/// promises a refusal only for a UUID an instance already holds. Constructed: a random or seeded
/// mint meets it with probability about rows / 2^53 per creation.
#[test]
fn adv_two_fresh_uuids_sharing_their_low_bits_do_not_refuse_a_creation() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut mint = answering(vec![
        Uuid::nil(),
        Uuid::from_u128(1 << 53),
        Uuid::from_u128(7),
    ]);
    let mut minted = BTreeSet::new();
    for name in ["first", "second"] {
        let opened = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            OPEN_CRATE,
            &object(json!({"name": name})),
        )
        .unwrap_or_else(|error| panic!("{name}: the crate opens: {error}"));
        let identity = opened.events[0].payload["crate_id"]
            .as_u64()
            .expect("an integer identity");
        assert!(minted.insert(identity), "{name}: {identity} again");
    }
}

/// Many creations from random UUIDv4s mint distinct identities, all within the safe range.
#[test]
fn adv_random_integer_identities_are_distinct() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut seen = BTreeSet::new();
    for index in 0..200 {
        let opened = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut UuidV4,
            OPEN_CRATE,
            &object(json!({"name": format!("crate {index}")})),
        )
        .unwrap_or_else(|error| panic!("crate {index} opens: {error}"));
        let number = opened.events[0].payload["crate_id"]
            .as_u64()
            .expect("an integer identity");
        assert!(number <= JSON_SAFE);
        assert!(seen.insert(number), "crate {index} repeats {number}");
    }
    assert_eq!(store.scan(CRATE).expect("the store scans").rows.len(), 200);
}

/// A minted `Integer` identity, sent back as a later command's `Integer` input, addresses the row
/// it was minted for; a number no crate carries takes the `unknown_instance:` branch.
#[test]
fn adv_a_minted_integer_identity_addresses_its_row_as_a_later_input() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let opened = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        OPEN_CRATE,
        &object(json!({"name": "first"})),
    )
    .expect("the crate opens");
    let identity = opened.events[0].payload["crate_id"].clone();
    assert!(identity.is_u64(), "{identity}");

    let renamed = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        RENAME_CRATE,
        &object(json!({"crate_id": identity, "name": "second"})),
    )
    .expect("the rename runs");
    assert_eq!(renamed.outcome, "renamed", "{renamed:?}");
    assert_eq!(renamed.events[0].payload["crate_id"], identity);
    let stored = store
        .load(CRATE, &identity)
        .expect("the store reads")
        .expect("the crate is still stored under its minted identity");
    assert_eq!(stored.row.fields.get("name"), Some(&json!("second")));

    let other = identity.as_u64().expect("an integer") ^ 1;
    let missing = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        RENAME_CRATE,
        &object(json!({"crate_id": other, "name": "third"})),
    )
    .expect("the rename of an unknown crate answers");
    assert_eq!(missing.outcome, "no-crate", "{missing:?}");
}

/// An `Integer` newtype that declares an invariant is not minted: the story mints only an
/// invariant-free one, and the mint (`value & (2^53 - 1)`) would break `value >= 1` on a draw.
#[test]
fn adv_an_integer_newtype_with_an_invariant_is_refused_at_start() {
    let ir = ir();
    match spindle_kernel::runs(&ir, spec(&ir, OPEN_TAGGED)) {
        Err(ExecuteError::Unsupported { construct }) => {
            assert!(construct.contains(OPEN_TAGGED), "{construct}");
            assert!(construct.contains("tag"), "{construct}");
            assert!(construct.contains("advw25.box.Tag"), "{construct}");
        }
        other => panic!("an Integer newtype with an invariant is refused at start, not {other:?}"),
    }
    let mut store = MemoryStore::default();
    let error = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        OPEN_TAGGED,
        &object(json!({"name": "t"})),
    )
    .expect_err("nothing is minted for a constrained Integer newtype");
    assert!(
        matches!(error, ExecuteError::Unsupported { .. }),
        "{error:?}"
    );
    assert!(
        store
            .scan("advw25.box.Tagged")
            .expect("scans")
            .rows
            .is_empty()
    );
}

// ---- fallbacks against the interpreter ------------------------------------------------------

/// What one step answered on either side: the outcome, the declared error with its fields, and
/// the events by name with their payloads, every value read as ESS's [`Node`].
#[derive(Debug, PartialEq)]
struct Step {
    outcome: String,
    error: Option<(String, BTreeMap<String, Node>)>,
    events: Vec<(String, BTreeMap<String, Node>)>,
}

/// Each step's answer, or its refusal, and the rows each view read after the steps.
type Answered = (Vec<Result<Step, String>>, Vec<Vec<BTreeMap<String, Node>>>);

fn nodes(map: &Map<String, Value>) -> BTreeMap<String, Node> {
    map.iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(field, value)| {
            (
                field.clone(),
                serde_json::from_value(value.clone())
                    .unwrap_or_else(|_| panic!("`{field}` holds a node, not {value}")),
            )
        })
        .collect()
}

/// `node` with every number spelled once: the interpreter writes an integral number as `7.0`
/// where the kernel writes `7`, and a decimal by its scale; a number compares by its value.
fn canonical(node: &Node) -> Node {
    match node {
        Node::Number(_) => {
            let text = serde_json::to_value(node)
                .expect("a node is JSON")
                .to_string();
            let value: f64 = text.trim_matches('"').parse().unwrap_or(f64::NAN);
            Node::Text(format!("number:{value}"))
        }
        Node::Map(map) => Node::Map(map.iter().map(|(k, v)| (k.clone(), canonical(v))).collect()),
        Node::Seq(items) => Node::Seq(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn canonical_map(map: &BTreeMap<String, Node>) -> BTreeMap<String, Node> {
    map.iter().map(|(k, v)| (k.clone(), canonical(v))).collect()
}

fn canonical_step(step: &Step) -> Step {
    Step {
        outcome: step.outcome.clone(),
        error: step
            .error
            .as_ref()
            .map(|(name, fields)| (name.clone(), canonical_map(fields))),
        events: step
            .events
            .iter()
            .map(|(name, payload)| (name.clone(), canonical_map(payload)))
            .collect(),
    }
}

/// Each step through ESS's interpreter, an `Err` where it answers none, and the rows of `views`
/// after the last.
fn interpreted(ir: &spindle_spec::EssIr, views: &[&str], steps: &[(&str, Value)]) -> Answered {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "advw25.box/authored/fallback"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("advw25").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps {
        let input = nodes(&object(input.clone()));
        let answer = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            })
            .map(|executed| Step {
                outcome: executed
                    .outcome
                    .map(|outcome| outcome.outcome.as_str().to_owned())
                    .unwrap_or_default(),
                error: executed
                    .error
                    .map(|error| (error.error.to_string(), error.fields)),
                events: executed
                    .direct_events
                    .iter()
                    .map(|event| (event.event.to_string(), event.payload.clone()))
                    .collect(),
            })
            .map_err(|error| error.to_string());
        answers.push(answer);
    }
    let rows = views
        .iter()
        .map(|view| {
            target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: BTreeMap::new(),
                    consistency: ess_primitives::consistency::QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
                })
                .expect("the interpreter reads the view")
                .rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .filter(|(_, node)| *node != Node::Null)
                        .collect::<BTreeMap<_, _>>()
                })
                .collect()
        })
        .collect();
    (answers, rows)
}

/// Each step through the kernel, an `Err` where it runs none, and each stored row of `entities`
/// (entity, identity field) as its view projects it.
fn spindled(
    ir: &spindle_spec::EssIr,
    entities: &[(&str, &str)],
    steps: &[(&str, Value)],
) -> Answered {
    let mut store = MemoryStore::default();
    let answers = steps
        .iter()
        .map(|(command, input)| {
            execute(
                ir,
                &mut store,
                &mut NoExternals,
                command,
                &object(input.clone()),
            )
            .map(|executed| Step {
                outcome: executed.outcome,
                error: executed
                    .error
                    .map(|error| (error.name, nodes(&error.fields))),
                events: executed
                    .events
                    .iter()
                    .map(|event| (event.name.clone(), nodes(&event.payload)))
                    .collect(),
            })
            .map_err(|error| error.to_string())
        })
        .collect();
    let rows = entities
        .iter()
        .map(|(entity, identity_field)| {
            store
                .scan(entity)
                .expect("the store scans")
                .into_iter()
                .map(|(identity, stored)| {
                    let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
                    row.insert((*identity_field).to_owned(), identity);
                    row.insert("state".to_owned(), json!(stored.row.state));
                    nodes(&row)
                })
                .collect()
        })
        .collect();
    (answers, rows)
}

fn sorted_rows(rows: &[BTreeMap<String, Node>]) -> Vec<BTreeMap<String, Node>> {
    let mut rows: Vec<_> = rows.iter().map(canonical_map).collect();
    rows.sort_by_key(|row| format!("{row:?}"));
    rows
}

/// Asserts both sides answered each step alike (a value by its number, not its spelling), and
/// hold the same rows.
fn agree(steps: &[(&str, Value)], ours: &Answered, theirs: &Answered) {
    for (index, ((command, input), (spindle, ess))) in
        steps.iter().zip(ours.0.iter().zip(&theirs.0)).enumerate()
    {
        match (spindle, ess) {
            (Ok(spindle), Ok(ess)) => assert_eq!(
                canonical_step(spindle),
                canonical_step(ess),
                "step {index} {command} {input}: the kernel answers what the interpreter answers"
            ),
            (Err(_), Err(_)) => {}
            (spindle, ess) => panic!(
                "step {index} {command} {input}: the kernel answers {spindle:?}, the interpreter \
                 {ess:?}"
            ),
        }
    }
    for (index, (spindle, ess)) in ours.1.iter().zip(&theirs.1).enumerate() {
        assert_eq!(
            sorted_rows(spindle),
            sorted_rows(ess),
            "view {index}: the kernel stores what the interpreter stores"
        );
    }
}

fn slip(slip_id: &str, extra: Value) -> Value {
    let mut input = object(json!({
        "slip_id": slip_id,
        "reject": false,
        "fallback_reason": "by default",
    }));
    input.extend(object(extra));
    Value::Object(input)
}

/// Each literal-spelled type falls back to its literal where the primary is absent, sent as
/// `null`, or (for the nested path) where its parent is absent or present without it; and every
/// primary sent present is read as sent. Both sides answer each request alike.
#[test]
fn adv_a_literal_fallback_of_each_type_answers_as_the_interpreter() {
    let ir = ir();
    let steps = [
        (FILE_SLIP, slip(SLIP, json!({}))),
        (
            FILE_SLIP,
            slip(
                SLIP_TWO,
                json!({
                    "address": {"city": "Lyon"},
                    "size": "small",
                    "weight": 7,
                    "fragile": false,
                }),
            ),
        ),
        (
            FILE_SLIP,
            slip(
                SLIP_THREE,
                json!({"address": {}, "size": null, "weight": null, "fragile": null}),
            ),
        ),
        (
            FILE_SLIP,
            slip(
                SLIP_FOUR,
                json!({"address": {"city": null}, "weight": 1.25}),
            ),
        ),
    ];
    let ours = spindled(&ir, &[("advw25.box.Slip", "slip_id")], &steps);
    let theirs = interpreted(&ir, &["advw25.box.SlipCard"], &steps);
    for (index, answer) in theirs.0.iter().enumerate() {
        assert!(
            answer.is_ok(),
            "the interpreter runs step {index}: {answer:?}"
        );
    }
    agree(&steps, &ours, &theirs);
    let first = ours.0[0].as_ref().expect("the kernel files the bare slip");
    let payload = canonical_map(&first.events[0].1);
    assert_eq!(payload["city"], Node::Text("nowhere".into()));
    assert_eq!(payload["size"], Node::Text("large".into()));
    assert_eq!(payload["fragile"], Node::Bool(true));
}

/// A creation identity that falls back to another input (`{input: backed_id, else:
/// input.backup_id}`): the primary where sent, else the other input; and a second creation under
/// a held fallback identity is not a new row on either side.
#[test]
fn adv_a_creation_identity_fallback_answers_as_the_interpreter() {
    let ir = ir();
    let steps = [
        (
            FILE_BACKED,
            json!({"backed_id": BACKED, "backup_id": BACKUP, "name": "primary"}),
        ),
        (
            FILE_BACKED,
            json!({"backup_id": OTHER_BACKUP, "name": "fallen"}),
        ),
        (
            FILE_BACKED,
            json!({"backed_id": null, "backup_id": OTHER_BACKUP, "name": "again"}),
        ),
    ];
    let ours = spindled(&ir, &[("advw25.box.Backed", "backed_id")], &steps);
    let theirs = interpreted(&ir, &["advw25.box.BackedCard"], &steps);
    assert!(theirs.0[0].is_ok() && theirs.0[1].is_ok(), "{:?}", theirs.0);
    agree(&steps, &ours, &theirs);
    let fallen = ours.0[1]
        .as_ref()
        .expect("the kernel files under the backup id");
    assert_eq!(
        fallen.events[0].1["backed_id"],
        Node::Text(OTHER_BACKUP.into())
    );
}

/// A refusal's fields that fall back to another input, and over an input path to a literal, are
/// read as ESS's interpreter reads them (`interpret::execute::error_value`).
#[test]
fn adv_an_error_field_fallback_answers_as_the_interpreter() {
    let ir = ir();
    let steps = [
        (FILE_SLIP, slip(SLIP, json!({"reject": true}))),
        (
            FILE_SLIP,
            slip(
                SLIP_TWO,
                json!({"reject": true, "reason": "torn", "address": {"city": "Oslo"}}),
            ),
        ),
        (
            FILE_SLIP,
            slip(
                SLIP_THREE,
                json!({"reject": true, "reason": null, "address": {"city": null}}),
            ),
        ),
    ];
    let ours = spindled(&ir, &[("advw25.box.Slip", "slip_id")], &steps);
    let theirs = interpreted(&ir, &["advw25.box.SlipCard"], &steps);
    for (index, answer) in theirs.0.iter().enumerate() {
        assert!(
            answer.is_ok(),
            "the interpreter runs step {index}: {answer:?}"
        );
    }
    agree(&steps, &ours, &theirs);
    let first = ours.0[0]
        .as_ref()
        .expect("the kernel refuses with the error");
    assert_eq!(
        first.error,
        Some((
            "advw25.box.Rejected".to_owned(),
            BTreeMap::from([
                ("city".to_owned(), Node::Text("nowhere".into())),
                ("reason".to_owned(), Node::Text("by default".into())),
            ])
        ))
    );
}

/// A literal after a plain input in an error's fields is not read by ESS's interpreter
/// (`error_value` answers `NotInterpreted`); the kernel refuses that outcome by name too, and the
/// command's other outcome still runs.
#[test]
fn adv_a_literal_fallback_after_a_plain_input_in_an_error_is_refused_like_the_interpreter() {
    let ir = ir();
    let steps = [
        (GRUMBLE, json!({"note": "x"})),
        (GRUMBLE, json!({"note": "y"})),
    ];
    let theirs = interpreted(&ir, &[], &steps);
    assert!(
        theirs.0[0].is_err(),
        "the interpreter reads no literal fallback after a plain input in an error: {:?}",
        theirs.0[0]
    );
    let mut store = MemoryStore::default();
    match execute(
        &ir,
        &mut store,
        &mut NoExternals,
        GRUMBLE,
        &object(json!({"note": "x"})),
    ) {
        Err(ExecuteError::Unsupported { construct }) => {
            assert!(construct.contains("note"), "{construct}");
        }
        other => panic!("the grumble's literal fallback is refused by name, not {other:?}"),
    }
    let fine = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        GRUMBLE,
        &object(json!({"note": "y"})),
    )
    .expect("the other outcome runs");
    assert_eq!(fine.outcome, "fine");
    assert_eq!(
        theirs.0[1].as_ref().map(|step| step.outcome.as_str()),
        Ok("fine")
    );
}
