//! Adversary cases against `spindle.deployment.DecideAdmission` (stories
//! `spindle-admission-rules-are-declared` and `admission-decision-checked-by-model`): each asserts
//! what `docs/design/spindle.md` § 3a and the outcome summaries in `ess/domains/deployment.yaml`
//! promise, run through the kernel.

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

/// Declares `laptop` providing `provides`, capability and implementation pairs declared as
/// `spindle.environment.Provision`s, then offers a bundle to it.
fn offer_providing(ir: &EssIr, store: &mut dyn Store, id: &str, provides: Value) {
    let declared = run(
        ir,
        store,
        &mut Answer::no(),
        "spindle.deployment.DeclareEnvironment",
        json!({
            "environment_id": "laptop",
            "os": "linux",
            "arch": "x86_64",
            "provides": crate::provides::provisions(&provides),
        }),
    );
    assert_eq!(declared.outcome, "declared", "{declared:?}");
    let offered = run(
        ir,
        store,
        &mut Answer::no(),
        "spindle.deployment.OfferBundle",
        json!({
            "deployment_id": id,
            "bundle": "sha256:0f",
            "environment_id": "laptop",
        }),
    );
    assert_eq!(offered.outcome, "offered", "{offered:?}");
}

/// Offers a bundle to `laptop`, which provides `clock` by `system` and `store.durable` by `sqlite`.
fn offer(ir: &EssIr, store: &mut dyn Store, id: &str) {
    offer_providing(
        ir,
        store,
        id,
        json!([
            {"capability": "clock", "implementation": "system"},
            {"capability": "store.durable", "implementation": "sqlite"},
        ]),
    );
}

fn decide(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    id: &str,
    requires: Value,
    bindings: Value,
    missing: Value,
) -> Executed {
    run(
        ir,
        store,
        decider,
        DECIDE,
        json!({
            "deployment_id": id,
            "requires": requires,
            "bindings": bindings,
            "missing": missing,
        }),
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

/// story:admission-decision-checked-by-model (H7) replaces
/// `adversary_admission_names_only_required_capabilities_missing_at_runtime`: that `missing` names
/// only required capabilities was a runtime rule, and a hand-written refusal naming `channel` was
/// taken. With `requires` the admitter's answer beside `missing`, the model checks it: the refusal
/// is `not-decided`, and its cause is never asked. `spindle_capability::admit` never writes one.
#[test]
fn adversary_a_refusal_naming_an_unrequired_capability_is_not_decided() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    let mut decider = Answer::yes();
    let decided = decide(
        &ir,
        &mut store,
        &mut decider,
        "d1",
        json!(["store.durable"]),
        json!([]),
        json!(["channel"]),
    );
    assert_eq!(
        (decided.outcome.as_str(), error_of(&decided)),
        ("not-decided", Some(INVALID)),
        "a refusal naming `channel`, which is not required, was taken: {decided:?}"
    );
    assert!(decider.asked.is_empty(), "{:?}", decider.asked);
    assert_eq!(state_of(&mut store, "d1"), "Offered");

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
/// refusal "names what is missing"; a capability the decision itself binds is not missing, so a
/// decider that answers `true` does not refuse the deployment.
#[test]
fn adversary_a_refusal_of_a_capability_the_decision_binds_is_invalid() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    let decided = decide(
        &ir,
        &mut store,
        &mut Answer::yes(),
        "d1",
        json!(["clock", "store.durable"]),
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

/// story:admission-decision-checked-by-model (H5) replaces
/// `adversary_admission_binds_only_what_the_environment_provides_at_runtime`: `admitted` did not
/// read `Environment.provides`, so binding `postgres` where `laptop` provides only `sqlite` was
/// admitted. The model now reads the deployment's environment and answers `not-provided`;
/// `spindle_capability::admit` still binds only what `provides` lists.
#[test]
fn adversary_a_binding_the_environment_does_not_provide_is_not_admitted() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer_providing(
        &ir,
        &mut store,
        "d1",
        json!([{"capability": "store.durable", "implementation": "sqlite"}]),
    );
    let decided = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!(["store.durable"]),
        json!([{"capability": "store.durable", "implementation": "postgres"}]),
        json!([]),
    );
    assert_eq!(
        (decided.outcome.as_str(), error_of(&decided)),
        ("not-provided", Some(INVALID)),
        "the model admitted an implementation `laptop` does not provide: {decided:?}"
    );
    assert_eq!(state_of(&mut store, "d1"), "Offered");
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

/// spindle.md § 3a: an inconsistent decision is refused from the request alone, before the held
/// state is read. On an admitted deployment a decision that binds nothing is `not-decided`
/// (`AdmissionDecisionInvalid`); a consistent one answers `wrong-state`.
#[test]
fn adversary_an_invalid_decision_on_a_decided_deployment_is_not_decided() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    let admitted = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!(["clock"]),
        json!([{"capability": "clock", "implementation": "system"}]),
        json!([]),
    );
    assert_eq!(admitted.outcome, "admitted", "{admitted:?}");
    let again = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!(["clock"]),
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
        json!(["clock"]),
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
        json!(["clock"]),
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

    offer(&ir, &mut store, "d1");
    let admitted = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!(["clock"]),
        json!([{"capability": "clock", "implementation": "system"}]),
        json!([]),
    );
    assert_eq!(admitted.outcome, "admitted", "{admitted:?}");
    let held = decide(
        &ir,
        &mut store,
        &mut Answer::no(),
        "d1",
        json!(["clock"]),
        twice,
        json!([]),
    );
    assert_eq!(
        (held.outcome.as_str(), error_of(&held)),
        ("bound-twice", Some(INVALID)),
        "{held:?}"
    );
    assert_eq!(state_of(&mut store, "d1"), "Admitted");
}

/// story:admission-decision-checked-by-model replaces `adversary_kernel_takes_the_synthesised_refusal`:
/// ESS 0.90 no longer synthesises `DecideAdmission/outcome/refused` (an `external:` branch beside a
/// `when_related:` refusal over a stored reference is not arranged, ESS-SYNTH-003), and the input
/// that scenario sent, `missing.0` outside `requires`, is now `not-decided`. The kernel takes the
/// consistent form of it, a refusal naming the one required capability and binding nothing, and
/// refuses the inconsistent ones.
#[test]
fn adversary_kernel_takes_a_consistent_refusal_and_no_other() {
    let ir = specification();
    let cases = [
        (json!([]), json!(["requires.0"]), "refused"),
        (json!([]), json!(["missing.0"]), "not-decided"),
        (
            json!([{"capability": "bindings.0.capability", "implementation": "bindings.0.implementation"}]),
            json!(["requires.0"]),
            "not-decided",
        ),
    ];
    for (bindings, missing, outcome) in cases {
        let mut store = MemoryStore::default();
        offer_providing(&ir, &mut store, "deployment_id", json!([]));
        let decided = decide(
            &ir,
            &mut store,
            &mut Answer::yes(),
            "deployment_id",
            json!(["requires.0"]),
            bindings,
            missing.clone(),
        );
        assert_eq!(decided.outcome, outcome, "{decided:?}");
        if outcome == "refused" {
            assert_eq!(
                Value::Object(decided.events[0].payload.clone()),
                json!({"deployment_id": "deployment_id", "missing": missing})
            );
            assert_eq!(state_of(&mut store, "deployment_id"), "Refused");
        } else {
            assert_eq!(error_of(&decided), Some(INVALID), "{decided:?}");
            assert_eq!(state_of(&mut store, "deployment_id"), "Offered");
        }
    }
}
