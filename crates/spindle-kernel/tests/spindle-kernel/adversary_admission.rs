//! Adversary cases against `spindle.deployment.DecideAdmission` (story
//! `spindle-admission-rules-are-declared`): each asserts what `docs/design/spindle.md` § 3a and the
//! outcome summaries in `ess/domains/deployment.yaml` promise, run through the kernel.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

const DEPLOYMENT: &str = "spindle.deployment.Deployment";
const DECIDE: &str = "spindle.deployment.DecideAdmission";
const INVALID: &str = "spindle.deployment.AdmissionDecisionInvalid";
const CONFLICT: &str = "spindle.deployment.DeploymentStateConflict";

fn specification() -> EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ess");
    spindle_spec::load(&root).expect("ess/ compiles").ir
}

/// Answers every external question with `answer` and records what it was asked.
struct Answer {
    answer: bool,
    asked: Vec<String>,
}

impl Answer {
    fn yes() -> Self {
        Self {
            answer: true,
            asked: Vec::new(),
        }
    }
    fn no() -> Self {
        Self {
            answer: false,
            asked: Vec::new(),
        }
    }
}

impl Decider for Answer {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        self.asked.push(question.capability.clone());
        Ok(self.answer)
    }
}

fn run(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    command: &str,
    request: Value,
) -> Executed {
    let Value::Object(map) = request else {
        panic!("an input is an object");
    };
    let map: Map<String, Value> = map;
    execute(ir, store, decider, command, &map)
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn offer(ir: &EssIr, store: &mut dyn Store, id: &str, requires: Value) {
    let offered = run(
        ir,
        store,
        &mut Answer::no(),
        "spindle.deployment.OfferBundle",
        json!({
            "deployment_id": id,
            "bundle": "sha256:0f",
            "environment_id": "laptop",
            "requires": requires,
        }),
    );
    assert_eq!(offered.outcome, "offered", "{offered:?}");
}

fn decide(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    id: &str,
    bindings: Value,
    missing: Value,
) -> Executed {
    run(
        ir,
        store,
        decider,
        DECIDE,
        json!({"deployment_id": id, "bindings": bindings, "missing": missing}),
    )
}

fn error_of(executed: &Executed) -> Option<&str> {
    executed.error.as_ref().map(|error| error.name.as_str())
}

fn state_of(store: &mut dyn Store, id: &str) -> String {
    store
        .load(DEPLOYMENT, &json!(id))
        .expect("the store reads")
        .expect("the deployment exists")
        .row
        .state
}

/// spindle.md § 3a: `missing` names only required capabilities, and that is a runtime rule, not a
/// guard: `spindle_capability::admit` builds `missing` from the requirements it is given, so the
/// runtime never sends a refusal naming a capability outside `requires`. The model does not check
/// it (a guard relating `input.missing` to the stored `requires` leaves `refused` unwitnessed by
/// synthesis), so a hand-written refusal naming `channel` is taken; `admit` never writes one.
#[test]
fn adversary_admission_names_only_required_capabilities_missing_at_runtime() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1", json!(["store.durable"]));
    let decided = decide(
        &ir,
        &mut store,
        &mut Answer::yes(),
        "d1",
        json!([]),
        json!(["channel"]),
    );
    assert_eq!(
        decided.outcome, "refused",
        "the model does not relate `missing` to `requires` (spindle.md § 3a): {decided:?}"
    );

    let environment = spindle_capability::Environment::parse(
        "format = \"spindle-environment/1\"\nname = \"bare\"\nos = \"linux\"\narch = \"x86_64\"\n\n[provides]\nchannel = [\"tcp\"]\n",
    )
    .expect("the environment parses");
    let requirements: spindle_capability::Requirements = [(
        spindle_capability::Capability::new("store.durable"),
        Default::default(),
    )]
    .into_iter()
    .collect();
    let spindle_capability::Admission::Refused { missing } =
        spindle_capability::admit(&requirements, &environment)
    else {
        panic!("refused expected");
    };
    assert_eq!(
        missing,
        [spindle_capability::Capability::new("store.durable")]
    );
    assert!(
        missing
            .iter()
            .all(|capability| requirements.contains_key(capability)),
        "admit names only required capabilities missing: {missing:?}"
    );
}

/// The decision binds `clock` and names `clock` missing in the same breath. spindle.md § 3a: a
/// refusal "names what is missing"; a capability the decision itself binds is not missing.
/// `admitted_binds_exactly_one_implementation_per_requirement`'s `admitted-but-missing` case sends
/// this input with a decider that answers `false`, so it passes only because the cause is
/// declined; a decider that answers `true` refuses the deployment.
#[test]
fn adversary_a_refusal_of_a_capability_the_decision_binds_is_invalid() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1", json!(["clock", "store.durable"]));
    let decided = decide(
        &ir,
        &mut store,
        &mut Answer::yes(),
        "d1",
        json!([
            {"capability": "store.durable", "implementation": "sqlite"},
            {"capability": "clock", "implementation": "system"},
        ]),
        json!(["clock"]),
    );
    assert_eq!(
        (decided.outcome.as_str(), error_of(&decided)),
        ("not-decided", Some(INVALID)),
        "a refusal of `clock`, which the same decision binds, was taken: {decided:?}"
    );
    assert_eq!(state_of(&mut store, "d1"), "Offered");
}

/// spindle.md § 3a: that the environment provides each binding is a runtime rule. `admitted` does
/// not read `Environment.provides`, so a decision binding `postgres` where `laptop` provides only
/// `sqlite` is admitted by the model; `spindle_capability::admit` binds only what `provides` lists.
#[test]
fn adversary_admission_binds_only_what_the_environment_provides_at_runtime() {
    let ir = specification();
    let mut store = MemoryStore::default();
    let declared = run(
        &ir,
        &mut store,
        &mut Answer::no(),
        "spindle.deployment.DeclareEnvironment",
        json!({
            "environment_id": "laptop",
            "os": "linux",
            "arch": "x86_64",
            "provides": [{"capability": "store.durable", "implementation": "sqlite"}],
        }),
    );
    assert_eq!(declared.outcome, "declared", "{declared:?}");
    offer(&ir, &mut store, "d1", json!(["store.durable"]));
    let decided = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!([{"capability": "store.durable", "implementation": "postgres"}]),
        json!([]),
    );
    assert_eq!(
        decided.outcome, "admitted",
        "the model does not read `provides` (spindle.md § 3a): {decided:?}"
    );
    let environment = spindle_capability::Environment::parse(
        "format = \"spindle-environment/1\"\nname = \"laptop\"\nos = \"linux\"\narch = \"x86_64\"\n\n[provides]\n\"store.durable\" = [\"sqlite\"]\n",
    )
    .expect("the environment parses");
    let requirements: spindle_capability::Requirements = [(
        spindle_capability::Capability::new("store.durable"),
        Default::default(),
    )]
    .into_iter()
    .collect();
    let spindle_capability::Admission::Admitted { bindings } =
        spindle_capability::admit(&requirements, &environment)
    else {
        panic!("admitted expected");
    };
    assert_eq!(
        bindings[0].implementation, "sqlite",
        "admit binds only what `provides` lists"
    );
}

/// spindle.md § 3a: ESS answers the state conflict only when the branch a decision selects moves
/// the deployment. On an admitted deployment a decision that binds nothing selects `not-decided`
/// (`AdmissionDecisionInvalid`), which does not move it; a valid decision selects `admitted`,
/// which does not move from `Admitted`, so it answers `wrong-state`.
#[test]
fn adversary_an_invalid_decision_on_a_decided_deployment_is_not_decided() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1", json!(["clock"]));
    let admitted = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!([{"capability": "clock", "implementation": "system"}]),
        json!([]),
    );
    assert_eq!(admitted.outcome, "admitted", "{admitted:?}");
    let again = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!([]),
        json!([]),
    );
    assert_eq!(
        (again.outcome.as_str(), error_of(&again)),
        ("not-decided", Some(INVALID)),
        "{again:?}"
    );
    assert_eq!(state_of(&mut store, "d1"), "Admitted");
    let valid = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!([{"capability": "clock", "implementation": "system"}]),
        json!([]),
    );
    assert_eq!(
        (valid.outcome.as_str(), error_of(&valid)),
        ("wrong-state", Some(CONFLICT)),
        "{valid:?}"
    );
    assert_eq!(state_of(&mut store, "d1"), "Admitted");
}

/// spindle.md § 3a: "a capability bound twice is refused before the deployment is read
/// (`bound-twice`)": for a deployment nobody offered and for one already admitted.
#[test]
fn adversary_bound_twice_answers_before_the_deployment_is_read() {
    let ir = specification();
    let twice = json!([
        {"capability": "clock", "implementation": "system"},
        {"capability": "clock", "implementation": "monotonic"},
    ]);
    let mut store = MemoryStore::default();
    let mut decider = Answer::yes();
    let unknown = decide(
        &ir,
        &mut store,
        &mut decider,
        "nowhere",
        twice.clone(),
        json!(["clock"]),
    );
    assert_eq!(
        (unknown.outcome.as_str(), error_of(&unknown)),
        ("bound-twice", Some(INVALID)),
        "{unknown:?}"
    );
    assert!(decider.asked.is_empty(), "{:?}", decider.asked);
    assert_eq!(store.load(DEPLOYMENT, &json!("nowhere")).unwrap(), None);

    offer(&ir, &mut store, "d1", json!(["clock"]));
    let admitted = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!([{"capability": "clock", "implementation": "system"}]),
        json!([]),
    );
    assert_eq!(admitted.outcome, "admitted", "{admitted:?}");
    let held = decide(&ir, &mut store, &mut Answer::no(), "d1", twice, json!([]));
    assert_eq!(
        (held.outcome.as_str(), error_of(&held)),
        ("bound-twice", Some(INVALID)),
        "{held:?}"
    );
    assert_eq!(state_of(&mut store, "d1"), "Admitted");
}

/// The synthesised `DecideAdmission/outcome/refused` scenario of `ess conform synthesize --path
/// ess`, replayed with `refused` forced: requires `[requires.0]`, misses `missing.0`, and binds
/// nothing (the suite at this specification) or `bindings.0.capability` (the suite at 06d8a96).
/// `refused` is eligible when `missing` is not empty and names no capability the decision binds;
/// `missing.0` is bound by neither, so ESS selects `refused` for both (that `missing.0` is outside
/// `requires` is a runtime rule, not a guard, spindle.md § 3a) and the kernel must too.
#[test]
fn adversary_kernel_takes_the_synthesised_refusal() {
    let ir = specification();
    for bindings in [
        json!([]),
        json!([{"capability": "bindings.0.capability", "implementation": "bindings.0.implementation"}]),
    ] {
        let mut store = MemoryStore::default();
        offer(&ir, &mut store, "deployment_id", json!(["requires.0"]));
        let decided = decide(
            &ir,
            &mut store,
            &mut Answer::yes(),
            "deployment_id",
            bindings,
            json!(["missing.0"]),
        );
        assert_eq!(decided.outcome, "refused", "{decided:?}");
        assert_eq!(
            Value::Object(decided.events[0].payload.clone()),
            json!({"deployment_id": "deployment_id", "missing": ["missing.0"]})
        );
        assert_eq!(state_of(&mut store, "deployment_id"), "Refused");
    }
}
