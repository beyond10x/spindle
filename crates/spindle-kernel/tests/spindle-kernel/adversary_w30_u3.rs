//! Adversary cases of wave 30 U3 (`story:unknown-instance-unsourced-unserved`,
//! `story:error-payload-generated-fields`): a declared error's values, `{generated: true}` or
//! given no source, of every type the runtime mints, draw nothing from the sequence identities are
//! minted from, across several commands and refusal paths in one run; and an `unknown_instance:`
//! refusal with a required field given no source is refused by name and draws nothing.
//!
//! The fixture `tests/fixtures/adversary_w30_u3` is a bay that makes crates (`Uuid` identity, a
//! generated `Boolean` published), slots (`Integer` identity) and tags (`String` identity). A
//! jammed request is refused with `Jammed`, whose `Integer`, `String`, `Timestamp`, `Boolean`,
//! `Uuid`, record (`Badge`) and map fields are each `{generated: true}`; a stalled one with
//! `Stalled`, the same fields with no source. `SealCrate` refuses a sealed crate, and a crate
//! nobody holds, through `wrong_state:` with `Stalled`. `TapCrate` declares an
//! `unknown_instance:` refusal whose error has an `Optional` `hint` and a required `crate_id`,
//! neither given a source.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Identity, MemoryStore, Mint, NoExternals, Seeded, Store, Uuid, execute_at,
};

const MAKE_CRATE: &str = "bay.yard.MakeCrate";
const SEAL_CRATE: &str = "bay.yard.SealCrate";
const TAP_CRATE: &str = "bay.yard.TapCrate";
const MAKE_SLOT: &str = "bay.yard.MakeSlot";
const MAKE_TAG: &str = "bay.yard.MakeTag";
/// A crate identity nobody holds.
const NOBODY: &str = "5d1c0a3e-8b2f-4c6d-9e7a-0f1b2c3d4e5f";

fn bay() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_w30_u3");
    spindle_spec::load(&root)
        .expect("the adversary_w30_u3 fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(fields) = value else {
        panic!("an input is an object, not {value}")
    };
    fields
}

/// One step of a run: a command, and its input, where `{crate}` in a `crate_id` names the crate
/// the run made first.
#[derive(Clone)]
struct Step {
    command: &'static str,
    input: Value,
}

fn step(command: &'static str, input: Value) -> Step {
    Step { command, input }
}

/// Every refusal path the kernel fills an error's fields on, among creations of every minted
/// identity type: a `when:` refusal with generated fields, one with unsourced fields, a
/// `wrong_state:` refusal for a held row and for a row nobody holds, and an `unknown_instance:`
/// refusal the kernel does not serve.
fn with_refusals() -> Vec<Step> {
    vec![
        step(
            MAKE_CRATE,
            json!({"label": "j", "jam": true, "stall": false}),
        ),
        step(
            MAKE_CRATE,
            json!({"label": "a", "jam": false, "stall": false}),
        ),
        step(MAKE_SLOT, json!({"label": "j", "jam": true})),
        step(MAKE_SLOT, json!({"label": "s", "jam": false})),
        step(SEAL_CRATE, json!({"crate_id": "{crate}"})),
        step(SEAL_CRATE, json!({"crate_id": "{crate}"})),
        step(SEAL_CRATE, json!({"crate_id": NOBODY})),
        step(MAKE_TAG, json!({"label": "q", "stall": true})),
        step(MAKE_TAG, json!({"label": "t", "stall": false})),
        step(
            MAKE_CRATE,
            json!({"label": "q", "jam": false, "stall": true}),
        ),
        step(
            MAKE_CRATE,
            json!({"label": "b", "jam": false, "stall": false}),
        ),
        step(TAP_CRATE, json!({"crate_id": NOBODY, "label": "x"})),
        step(MAKE_SLOT, json!({"label": "u", "jam": false})),
        step(
            MAKE_CRATE,
            json!({"label": "c", "jam": false, "stall": false}),
        ),
    ]
}

/// [`with_refusals`] with every refused step left out: the same creations, in the same order.
fn without_refusals() -> Vec<Step> {
    vec![
        step(
            MAKE_CRATE,
            json!({"label": "a", "jam": false, "stall": false}),
        ),
        step(MAKE_SLOT, json!({"label": "s", "jam": false})),
        step(SEAL_CRATE, json!({"crate_id": "{crate}"})),
        step(MAKE_TAG, json!({"label": "t", "stall": false})),
        step(
            MAKE_CRATE,
            json!({"label": "b", "jam": false, "stall": false}),
        ),
        step(MAKE_SLOT, json!({"label": "u", "jam": false})),
        step(
            MAKE_CRATE,
            json!({"label": "c", "jam": false, "stall": false}),
        ),
    ]
}

/// `input` with `{crate}` replaced by `first`.
fn resolved(input: &Value, first: Option<&Value>) -> Value {
    let mut input = input.clone();
    if input["crate_id"] == "{crate}" {
        input["crate_id"] = first.cloned().expect("a crate was made first");
    }
    input
}

/// What one step answered, as the comparison reads it: a creation's published values, a
/// refusal's error name and fields, a move's outcome, or `unsupported` with what was named.
fn answered(executed: Result<spindle_kernel::Executed, ExecuteError>) -> Value {
    match executed {
        Ok(executed) => match (executed.error, executed.events.first()) {
            (Some(error), _) => json!({"refused": error.name, "fields": error.fields}),
            (None, Some(event)) => json!({"made": event.name, "payload": event.payload}),
            (None, None) => json!({"took": executed.outcome}),
        },
        Err(ExecuteError::Unsupported { construct }) => json!({"unsupported": construct}),
        Err(other) => panic!("the step runs: {other:?}"),
    }
}

/// A fixed clock, so every run answers the same `Timestamp`.
fn fixed() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

/// The kernel's run of `steps` on one store under `mint`: what each answered.
fn kernel_run(ir: &spindle_spec::EssIr, mint: &mut dyn Mint, steps: &[Step]) -> Vec<Value> {
    let mut store = MemoryStore::default();
    let mut first = None;
    steps
        .iter()
        .map(|step| {
            let input = resolved(&step.input, first.as_ref());
            let answer = answered(execute_at(
                ir,
                &mut store,
                &mut NoExternals,
                &mut *mint,
                &fixed,
                step.command,
                &object(input),
            ));
            if first.is_none()
                && let Some(id) = answer["payload"].get("crate_id")
            {
                first = Some(id.clone());
            }
            answer
        })
        .collect()
}

/// The creations of a run, in order: what each published.
fn creations(run: &[Value]) -> Vec<Value> {
    run.iter()
        .filter(|answer| answer.get("made").is_some())
        .cloned()
        .collect()
}

/// The refusals of a run, in order: the error and its fields.
fn refusals(run: &[Value]) -> Vec<Value> {
    run.iter()
        .filter(|answer| answer.get("refused").is_some())
        .cloned()
        .collect()
}

/// A mint that answers the UUID ESS's interpreter mints on each tick in turn, and records what
/// asked for each.
struct Ticks {
    ir: spindle_spec::EssIr,
    tick: u64,
    asked: Vec<String>,
}

impl Ticks {
    fn new(ir: &spindle_spec::EssIr) -> Self {
        Self {
            ir: ir.clone(),
            tick: 0,
            asked: Vec::new(),
        }
    }
}

impl Mint for Ticks {
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid {
        self.tick += 1;
        self.asked
            .push(format!("{}.{}", identity.entity, identity.field));
        let uuid = ess_compiler::ir::ResolvedTypeRef::Primitive {
            name: ess_domain::types::Primitive::Uuid,
        };
        match ess_conformance::interpret::values::mint(&self.ir, &uuid, self.tick) {
            Ok(Some(Node::Text(text))) => Uuid::parse_str(&text).expect("ESS mints a UUID"),
            other => panic!("ESS mints a UUID on tick {}: {other:?}", self.tick),
        }
    }
}

/// What ESS's interpreter publishes for each crate made in `steps`, in one scenario: the
/// `CrateMade` payload, as JSON.
fn interpreted_crates(ir: &spindle_spec::EssIr, steps: &[Step]) -> Vec<Value> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "bay.yard/authored/adversary-w30-u3"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w30-u3").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut first: Option<Value> = None;
    let mut crates = Vec::new();
    for step in steps {
        let input: BTreeMap<String, Node> =
            serde_json::from_value(resolved(&step.input, first.as_ref()))
                .expect("the input as ESS values");
        let result = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(step.command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("the interpreter runs {}: {error:?}", step.command));
        if let Some(event) = result
            .direct_events
            .iter()
            .find(|event| event.event.to_string() == "bay.yard.CrateMade")
        {
            let payload = serde_json::to_value(&event.payload).expect("a payload is JSON");
            if first.is_none() {
                first = Some(payload["crate_id"].clone());
            }
            crates.push(payload);
        }
    }
    crates
}

/// Under a mint that answers ESS's ticks, every crate made after refusals of every path and of
/// every error field type is the crate ESS's interpreter makes, its identity and its generated
/// `lit` alike; and the slots and tags, whose `Integer` and `String` identities are drawn from
/// the same sequence, are those of the same run without the refusals.
#[test]
fn every_refusal_path_and_error_field_type_draws_as_ess_draws() {
    let ir = bay();
    let mut ticks = Ticks::new(&ir);
    let spindle = kernel_run(&ir, &mut ticks, &with_refusals());
    let refused = refusals(&spindle);
    assert_eq!(
        refused.len(),
        6,
        "every refusal path is taken: {spindle:#?}"
    );
    for refusal in &refused {
        let fields = refusal["fields"]
            .as_object()
            .unwrap_or_else(|| panic!("{refusal}"));
        for field in ["code", "note", "seen", "loud", "ticket", "badge", "marks"] {
            assert!(
                fields.contains_key(field),
                "`{field}` is filled on {refusal}"
            );
        }
    }
    assert!(
        spindle.iter().any(|answer| answer["unsupported"]
            .as_str()
            .is_some_and(|named| named.contains("bay.yard.Missing.crate_id")
                && named.contains("bay.yard.TapCrate/no-crate"))),
        "the unknown-instance refusal with an unsourced required field is refused by name: \
         {spindle:#?}"
    );
    assert!(
        ticks
            .asked
            .iter()
            .all(|asked| !asked.starts_with("bay.yard.Jammed")
                && !asked.starts_with("bay.yard.Stalled")
                && !asked.starts_with("bay.yard.Missing")),
        "no error field draws from the run's mint: {:?}",
        ticks.asked
    );

    let ess = interpreted_crates(&ir, &with_refusals());
    let made = creations(&spindle);
    let crates = made
        .iter()
        .filter(|made| made["made"] == "bay.yard.CrateMade")
        .map(|made| made["payload"].clone())
        .collect::<Vec<_>>();
    assert_eq!(crates.len(), 3, "{made:#?}");
    assert_eq!(crates, ess, "the crates are minted on ESS's ticks");

    let alone = kernel_run(&ir, &mut Ticks::new(&ir), &without_refusals());
    assert_eq!(
        made,
        creations(&alone),
        "slots and tags are minted as in the run without refusals"
    );
}

/// Under a seeded mint, the creations of a run with refusals of every path are those of the same
/// run without them, and one seed answers the same error values on every run.
#[test]
fn a_seeded_mint_keeps_every_creation_in_step_across_every_refusal_path() {
    let ir = bay();
    for seed in [0, 1, 7, u64::MAX] {
        let with = kernel_run(&ir, &mut Seeded::new(seed), &with_refusals());
        let without = kernel_run(&ir, &mut Seeded::new(seed), &without_refusals());
        assert_eq!(
            creations(&with),
            creations(&without),
            "seed {seed}: the refusals move no identity"
        );
        let again = kernel_run(&ir, &mut Seeded::new(seed), &with_refusals());
        assert_eq!(
            refusals(&again),
            refusals(&with),
            "seed {seed}: one seed answers the same error values"
        );
    }
}

/// A seeded mint's error values are not one value answered again: each refusal draws its own.
#[test]
fn a_seeded_mint_answers_each_refusal_its_own_values() {
    let ir = bay();
    let run = kernel_run(&ir, &mut Seeded::new(3), &with_refusals());
    let tickets = refusals(&run)
        .iter()
        .map(|refusal| refusal["fields"]["ticket"].to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        tickets.len(),
        6,
        "each refusal mints its own ticket: {run:#?}"
    );
}

/// The required error field an `unknown_instance:` refusal gives no source is the one named, not
/// the `Optional` one declared before it; nothing is written and nothing is drawn.
#[test]
fn an_unserved_unknown_instance_refusal_names_the_required_field_and_draws_nothing() {
    let ir = bay();
    let mut ticks = Ticks::new(&ir);
    let mut store = MemoryStore::default();
    let refused = execute_at(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut ticks,
        &fixed,
        TAP_CRATE,
        &object(json!({"crate_id": NOBODY, "label": "x"})),
    );
    match refused {
        Err(ExecuteError::Unsupported { construct }) => {
            assert!(
                construct.contains("bay.yard.Missing.crate_id")
                    && construct.contains("bay.yard.TapCrate/no-crate")
                    && !construct.contains("hint"),
                "{construct}"
            );
        }
        other => panic!("the refusal is not served: {other:?}"),
    }
    assert!(ticks.asked.is_empty(), "{:?}", ticks.asked);
    assert!(
        store
            .scan("bay.yard.Crate")
            .expect("the store scans")
            .rows
            .is_empty(),
        "nothing is written"
    );
}
