//! Spindle's own specification, `spindle.deployment`, run by the kernel.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, SqliteStore, Store, execute};
use spindle_spec::EssIr;

const DEPLOYMENT: &str = "spindle.deployment.Deployment";
const REFUSED: &str = "external.spindle.deployment.DecideAdmission.refused";

fn specification() -> EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ess");
    spindle_spec::load(&root).expect("ess/ compiles").ir
}

fn input(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("an input is an object, not {other}"),
    }
}

/// Answers every external question `false` and records what it was asked.
#[derive(Default)]
struct Never {
    asked: Vec<String>,
}

impl Decider for Never {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        self.asked.push(question.capability.clone());
        Ok(false)
    }
}

fn run(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    command: &str,
    request: Value,
) -> Executed {
    execute(ir, store, decider, command, &input(request))
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

/// Declares `environment` providing `provides`, a list of capability and implementation pairs,
/// as `spindle.environment.Provision`s.
fn declare(ir: &EssIr, store: &mut dyn Store, environment: &str, provides: Value) {
    let declared = run(
        ir,
        store,
        &mut Never::default(),
        "spindle.deployment.DeclareEnvironment",
        json!({
            "environment_id": environment,
            "os": "linux",
            "arch": "x86_64",
            "provides": crate::provides::provisions(&provides),
        }),
    );
    assert_eq!(declared.outcome, "declared", "{declared:?}");
}

/// Declares `laptop`, providing `store.durable` by `sqlite` and `clock` by `system`.
fn laptop(ir: &EssIr, store: &mut dyn Store) {
    declare(
        ir,
        store,
        "laptop",
        json!([
            {"capability": "store.durable", "implementation": "sqlite"},
            {"capability": "clock", "implementation": "system"},
        ]),
    );
}

/// Offers a bundle to `laptop`, declaring it first.
fn offer(ir: &EssIr, store: &mut dyn Store, id: &str) -> Executed {
    laptop(ir, store);
    offer_to(ir, store, id, "laptop")
}

fn offer_to(ir: &EssIr, store: &mut dyn Store, id: &str, environment: &str) -> Executed {
    run(
        ir,
        store,
        &mut Never::default(),
        "spindle.deployment.OfferBundle",
        json!({
            "deployment_id": id,
            "bundle": "sha256:0f",
            "environment_id": environment,
        }),
    )
}

fn admit(ir: &EssIr, store: &mut dyn Store, decider: &mut dyn Decider, id: &str) -> Executed {
    decide(
        ir,
        store,
        decider,
        id,
        json!(["store.durable"]),
        json!([{"capability": "store.durable", "implementation": "sqlite"}]),
        json!([]),
    )
}

fn state_of(store: &mut dyn Store, id: &str) -> String {
    store
        .load(DEPLOYMENT, &json!(id))
        .expect("the store reads")
        .expect("the deployment exists")
        .row
        .state
}

#[test]
fn offer_then_admit_moves_a_deployment_to_admitted() {
    let ir = specification();
    let mut store = MemoryStore::default();

    let offered = offer(&ir, &mut store, "d1");
    assert_eq!(offered.outcome, "offered");
    assert_eq!(offered.error, None);
    assert_eq!(offered.events.len(), 1);
    assert_eq!(offered.events[0].name, "spindle.deployment.BundleOffered");
    assert_eq!(
        Value::Object(offered.events[0].payload.clone()),
        json!({"deployment_id": "d1", "bundle": "sha256:0f"})
    );
    assert_eq!(state_of(&mut store, "d1"), "Offered");
    let offered_row = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(
        offered_row.row.fields.get("requires"),
        None,
        "the offer names no capability: {offered_row:?}"
    );

    let mut decider = Never::default();
    let admitted = admit(&ir, &mut store, &mut decider, "d1");
    assert_eq!(admitted.outcome, "admitted");
    assert_eq!(admitted.error, None);
    assert!(
        decider.asked.is_empty(),
        "a decision naming nothing missing is not eligible for `refused`: {:?}",
        decider.asked
    );
    assert_eq!(admitted.events.len(), 1);
    assert_eq!(
        admitted.events[0].name,
        "spindle.deployment.DeploymentAdmitted"
    );
    assert_eq!(
        Value::Object(admitted.events[0].payload.clone()),
        json!({"deployment_id": "d1"})
    );

    let stored = store
        .load(DEPLOYMENT, &json!("d1"))
        .expect("the store reads")
        .expect("the deployment exists");
    assert_eq!(stored.row.state, "Admitted");
    assert_eq!(
        stored.row.fields.get("bindings"),
        Some(&json!([{"capability": "store.durable", "implementation": "sqlite"}]))
    );
    assert_eq!(
        stored.row.fields.get("requires"),
        Some(&json!(["store.durable"])),
        "`requires` is the admitter's answer, kept with the decision"
    );
    assert_eq!(stored.row.fields.get("bundle"), Some(&json!("sha256:0f")));
}

#[test]
fn an_external_cause_that_holds_refuses_the_deployment() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(&ir, &mut store, "bare", json!([]));
    offer_to(&ir, &mut store, "d1", "bare");

    let mut lacking =
        |question: &External<'_>| -> Result<bool, String> { Ok(question.capability == REFUSED) };
    let refused = decide(
        &ir,
        &mut store,
        &mut lacking,
        "d1",
        json!(["store.durable"]),
        json!([]),
        json!(["store.durable"]),
    );
    assert_eq!(refused.outcome, "refused");
    assert_eq!(
        refused.events[0].name,
        "spindle.deployment.DeploymentRefused"
    );
    let stored = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(stored.row.state, "Refused");
    assert_eq!(
        stored.row.fields.get("missing"),
        Some(&json!(["store.durable"]))
    );
    assert_eq!(
        stored.row.fields.get("requires"),
        Some(&json!(["store.durable"]))
    );
}

#[test]
fn admitting_twice_is_the_wrong_state_error() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    admit(&ir, &mut store, &mut Never::default(), "d1");
    let before = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();

    let mut decider = Never::default();
    let again = admit(&ir, &mut store, &mut decider, "d1");
    assert_eq!(again.outcome, "wrong-state");
    assert_eq!(
        again.error.as_ref().map(|error| error.name.as_str()),
        Some("spindle.deployment.DeploymentStateConflict")
    );
    assert!(again.events.is_empty(), "{again:?}");
    // A consistent decision on a deployment already decided answers the held state before any
    // external branch, so `refused`'s cause is never asked.
    assert!(decider.asked.is_empty(), "{:?}", decider.asked);
    assert_eq!(
        store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap(),
        before
    );

    let mut refusing = |_: &spindle_kernel::External<'_>| Ok::<bool, String>(true);
    let refused = admit(&ir, &mut store, &mut refusing, "d1");
    assert_eq!(refused.outcome, "wrong-state", "{refused:?}");
    assert_eq!(
        store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap(),
        before
    );
}

#[test]
fn stop_on_a_stopped_deployment_changes_nothing() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    admit(&ir, &mut store, &mut Never::default(), "d1");
    let started = run(
        &ir,
        &mut store,
        &mut Never::default(),
        "spindle.deployment.StartDeployment",
        json!({"deployment_id": "d1"}),
    );
    assert_eq!(started.outcome, "started");
    let stopped = run(
        &ir,
        &mut store,
        &mut Never::default(),
        "spindle.deployment.StopDeployment",
        json!({"deployment_id": "d1"}),
    );
    assert_eq!(stopped.outcome, "stopped");
    assert_eq!(
        stopped.events[0].name,
        "spindle.deployment.DeploymentStopped"
    );
    let before = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(before.row.state, "Stopped");

    let again = run(
        &ir,
        &mut store,
        &mut Never::default(),
        "spindle.deployment.StopDeployment",
        json!({"deployment_id": "d1"}),
    );
    assert_eq!(again.outcome, "already-ended");
    assert_eq!(again.error, None, "refuses: false succeeds");
    assert!(again.events.is_empty(), "{again:?}");
    assert_eq!(
        store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap(),
        before
    );
}

#[test]
fn starting_an_unknown_deployment_is_the_wrong_state_error() {
    let ir = specification();
    let mut store = MemoryStore::default();
    let started = run(
        &ir,
        &mut store,
        &mut Never::default(),
        "spindle.deployment.StartDeployment",
        json!({"deployment_id": "nowhere"}),
    );
    assert_eq!(started.outcome, "wrong-state");
    assert_eq!(
        started.error.map(|error| error.name),
        Some("spindle.deployment.DeploymentStateConflict".to_owned())
    );
    assert_eq!(store.load(DEPLOYMENT, &json!("nowhere")).unwrap(), None);
}

#[test]
fn state_survives_a_restart_on_sqlite() {
    let ir = specification();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("state.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("the store opens");
        offer(&ir, &mut store, "d1");
        admit(&ir, &mut store, &mut Never::default(), "d1");
        assert_eq!(state_of(&mut store, "d1"), "Admitted");
    }

    let mut reopened = SqliteStore::open(&path).expect("the store reopens");
    let stored = reopened
        .load(DEPLOYMENT, &json!("d1"))
        .expect("the store reads")
        .expect("the deployment survived");
    assert_eq!(stored.row.state, "Admitted");
    assert_eq!(stored.row.fields.get("bundle"), Some(&json!("sha256:0f")));

    let again = admit(&ir, &mut reopened, &mut Never::default(), "d1");
    assert_eq!(again.outcome, "wrong-state");
    let started = run(
        &ir,
        &mut reopened,
        &mut Never::default(),
        "spindle.deployment.StartDeployment",
        json!({"deployment_id": "d1"}),
    );
    assert_eq!(started.outcome, "started");
    assert_eq!(state_of(&mut reopened, "d1"), "Running");
}

const INVALID: &str = "spindle.deployment.AdmissionDecisionInvalid";

/// Answers every external question `true` and records what it was asked.
#[derive(Default)]
struct Always {
    asked: Vec<String>,
}

impl Decider for Always {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        self.asked.push(question.capability.clone());
        Ok(true)
    }
}

fn workspace() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
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
        "spindle.deployment.DecideAdmission",
        json!({
            "deployment_id": id,
            "requires": requires,
            "bindings": bindings,
            "missing": missing,
        }),
    )
}

/// `executed` is the invalid-decision refusal and left the deployment as it was offered.
fn assert_invalid(executed: &Executed, outcome: &str, store: &mut dyn Store, id: &str) {
    assert_eq!(executed.outcome, outcome, "{executed:?}");
    assert_eq!(
        executed.error.as_ref().map(|error| error.name.as_str()),
        Some(INVALID),
        "{executed:?}"
    );
    assert!(executed.events.is_empty(), "{executed:?}");
    let stored = store.load(DEPLOYMENT, &json!(id)).unwrap().unwrap();
    assert_eq!(stored.row.state, "Offered");
    assert_eq!(stored.row.fields.get("requires"), None, "{stored:?}");
    assert_eq!(stored.row.fields.get("bindings"), None, "{stored:?}");
    assert_eq!(stored.row.fields.get("missing"), None, "{stored:?}");
}

fn names(requirements: &spindle_capability::Requirements) -> Vec<String> {
    requirements
        .keys()
        .map(|capability| capability.as_str().to_owned())
        .collect()
}

fn environment(name: &str) -> spindle_capability::Environment {
    let path = workspace().join("examples/environments").join(name);
    let text = std::fs::read_to_string(&path).expect("the environment reads");
    spindle_capability::Environment::parse(&text).expect("the environment parses")
}

fn bindings_of(admission: &spindle_capability::Admission) -> Value {
    let spindle_capability::Admission::Admitted { bindings } = admission else {
        panic!("admitted expected, got {admission:?}");
    };
    bindings
        .iter()
        .map(|binding| {
            json!({
                "capability": binding.capability.as_str(),
                "implementation": binding.implementation,
            })
        })
        .collect()
}

#[test]
fn admitted_binds_exactly_one_implementation_per_requirement() {
    let ir = specification();
    let required = json!(["clock", "store.durable"]);
    let durable = json!({"capability": "store.durable", "implementation": "sqlite"});
    let clock = json!({"capability": "clock", "implementation": "system"});
    let cases = [
        ("unbound", json!([durable]), json!([]), "not-decided"),
        (
            "twice",
            json!([durable, {"capability": "store.durable", "implementation": "memory"}, clock]),
            json!([]),
            "bound-twice",
        ),
        (
            "unrequired",
            json!([durable, clock, {"capability": "channel", "implementation": "tcp"}]),
            json!([]),
            "not-decided",
        ),
        (
            "admitted-but-missing",
            json!([durable, clock]),
            json!(["clock"]),
            "not-decided",
        ),
    ];
    for (id, bindings, missing, outcome) in cases {
        let mut store = MemoryStore::default();
        offer(&ir, &mut store, id);
        let mut decider = Always::default();
        let decided = decide(
            &ir,
            &mut store,
            &mut decider,
            id,
            required.clone(),
            bindings,
            missing,
        );
        assert_invalid(&decided, outcome, &mut store, id);
        assert!(decider.asked.is_empty(), "{id}: {:?}", decider.asked);
    }

    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "exact");
    let admitted = decide(
        &ir,
        &mut store,
        &mut Never::default(),
        "exact",
        required,
        json!([clock, durable]),
        json!([]),
    );
    assert_eq!(admitted.outcome, "admitted", "{admitted:?}");
    let stored = store.load(DEPLOYMENT, &json!("exact")).unwrap().unwrap();
    assert_eq!(stored.row.state, "Admitted");
    assert_eq!(
        stored.row.fields.get("bindings"),
        Some(&json!([clock, durable]))
    );
}

#[test]
fn deployment_refused_carries_the_missing_capabilities() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(
        &ir,
        &mut store,
        "clock-only",
        json!([{"capability": "clock", "implementation": "system"}]),
    );
    offer_to(&ir, &mut store, "d1", "clock-only");
    let mut lacking =
        |question: &External<'_>| -> Result<bool, String> { Ok(question.capability == REFUSED) };
    let refused = decide(
        &ir,
        &mut store,
        &mut lacking,
        "d1",
        json!(["clock", "store.durable"]),
        json!([{"capability": "clock", "implementation": "system"}]),
        json!(["store.durable"]),
    );
    assert_eq!(refused.outcome, "refused", "{refused:?}");
    assert_eq!(refused.events.len(), 1);
    assert_eq!(
        refused.events[0].name,
        "spindle.deployment.DeploymentRefused"
    );
    assert_eq!(
        Value::Object(refused.events[0].payload.clone()),
        json!({"deployment_id": "d1", "missing": ["store.durable"]})
    );
    let stored = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(stored.row.state, "Refused");
    assert_eq!(
        stored.row.fields.get("missing"),
        Some(&json!(["store.durable"]))
    );
}

#[test]
fn a_refusal_naming_no_capability_is_refused() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    let mut refusing = Always::default();
    let decided = decide(
        &ir,
        &mut store,
        &mut refusing,
        "d1",
        json!(["store.durable"]),
        json!([]),
        json!([]),
    );
    assert_invalid(&decided, "not-decided", &mut store, "d1");
    assert!(
        refusing.asked.is_empty(),
        "a refusal naming nothing is not eligible, so its cause is never asked: {:?}",
        refusing.asked
    );
}

/// story:admission-decision-checked-by-model (H5): `admitted` reads the environment's `provides`.
/// A binding to an implementation the deployment's environment does not provide for the capability,
/// or to a capability it does not provide at all, is not admitted, whatever an external cause says.
#[test]
fn a_binding_the_environment_does_not_provide_is_not_admitted() {
    let ir = specification();
    let cases = [
        (
            "other-implementation",
            json!(["store.durable"]),
            json!([{"capability": "store.durable", "implementation": "postgres"}]),
            json!([]),
        ),
        (
            "unprovided-capability",
            json!(["channel"]),
            json!([{"capability": "channel", "implementation": "tcp"}]),
            json!([]),
        ),
        (
            "refusal-binding-another",
            json!(["channel", "store.durable"]),
            json!([{"capability": "store.durable", "implementation": "memory"}]),
            json!(["channel"]),
        ),
    ];
    for (id, requires, bindings, missing) in cases {
        let refuses = missing
            .as_array()
            .is_some_and(|missing| !missing.is_empty());
        let mut store = MemoryStore::default();
        offer(&ir, &mut store, id);
        let mut decider = Always::default();
        let decided = decide(
            &ir,
            &mut store,
            &mut decider,
            id,
            requires,
            bindings,
            missing,
        );
        assert_invalid(&decided, "not-provided", &mut store, id);
        // A refusing decision makes `refused` eligible: the held-state look-ahead, as ESS's
        // `select` reads it, asks its cause before the environment's row is read, and the
        // environment's refusal still answers. A decision naming nothing missing never asks it.
        if !refuses {
            assert!(decider.asked.is_empty(), "{id}: {:?}", decider.asked);
        }
    }
}

/// story:admission-decision-checked-by-model (H7): a refusal `AdmissionDecisionInvalid`'s own text
/// calls invalid is not answered `refused`, even where the external cause holds.
#[test]
fn a_refusal_the_invalid_decision_error_describes_is_not_refused() {
    let ir = specification();
    let durable = json!({"capability": "store.durable", "implementation": "sqlite"});
    let cases = [
        (
            "missing-unrequired",
            json!(["store.durable"]),
            json!([]),
            json!(["channel"]),
        ),
        (
            "required-left-out",
            json!(["clock", "store.durable"]),
            json!([]),
            json!(["store.durable"]),
        ),
        (
            "binds-unrequired",
            json!(["channel"]),
            json!([durable]),
            json!(["channel"]),
        ),
        (
            "missing-and-bound",
            json!(["store.durable"]),
            json!([durable]),
            json!(["store.durable"]),
        ),
    ];
    for (id, requires, bindings, missing) in cases {
        let mut store = MemoryStore::default();
        offer(&ir, &mut store, id);
        let mut decider = Always::default();
        let decided = decide(
            &ir,
            &mut store,
            &mut decider,
            id,
            requires,
            bindings,
            missing,
        );
        assert_invalid(&decided, "not-decided", &mut store, id);
        assert!(decider.asked.is_empty(), "{id}: {:?}", decider.asked);
    }
}

/// story:admission-decision-checked-by-model: a refusal naming missing a capability the deployment's
/// environment provides is `missing-provided`, even where the external cause holds; `laptop`
/// provides `store.durable` and `clock`.
#[test]
fn a_refusal_naming_a_provided_capability_missing_is_not_refused() {
    let ir = specification();
    let cases = [
        ("provided-durable", json!(["store.durable"]), json!([])),
        (
            "provided-beside-lacking",
            json!(["channel", "clock"]),
            json!([]),
        ),
    ];
    for (id, missing, bindings) in cases {
        let mut store = MemoryStore::default();
        offer(&ir, &mut store, id);
        let decided = decide(
            &ir,
            &mut store,
            &mut Always::default(),
            id,
            missing.clone(),
            bindings,
            missing,
        );
        assert_invalid(&decided, "missing-provided", &mut store, id);
    }
}

/// story:environment-implementations-specified: a capability is provided by its exact key, else by
/// a `prefix.*` key that covers it (README.md:101-102). An exact key that does not list the bound
/// implementation is `not-provided` though a prefix key would; a binding or a capability named
/// missing that only a prefix key covers is neither `not-provided` nor `missing-provided`.
#[test]
fn a_capability_only_a_prefix_key_covers_is_not_refused_as_unprovided() {
    let ir = specification();
    let cases = [
        (
            "prefix-binding",
            json!(["external.lobby.Join.full"]),
            json!([{"capability": "external.lobby.Join.full", "implementation": "operator-prompt"}]),
            json!([]),
            Some("admitted"),
        ),
        (
            "prefix-beside-exact",
            json!(["store.events"]),
            json!([{"capability": "store.events", "implementation": "memory"}]),
            json!([]),
            Some("admitted"),
        ),
        (
            "exact-wins",
            json!(["store.durable"]),
            json!([{"capability": "store.durable", "implementation": "memory"}]),
            json!([]),
            Some("not-provided"),
        ),
        (
            "prefix-missing",
            json!(["external.lobby.Join.full"]),
            json!([]),
            json!(["external.lobby.Join.full"]),
            None,
        ),
    ];
    for (id, requires, bindings, missing, outcome) in cases {
        let mut store = MemoryStore::default();
        declare(
            &ir,
            &mut store,
            "edge",
            json!([
                {"capability": "store.durable", "implementation": "sqlite"},
                {"capability": "store.*", "implementation": "memory"},
                {"capability": "external.*", "implementation": "operator-prompt"},
            ]),
        );
        let offered = offer_to(&ir, &mut store, id, "edge");
        assert_eq!(offered.outcome, "offered", "{offered:?}");
        let decided = decide(
            &ir,
            &mut store,
            &mut Always::default(),
            id,
            requires,
            bindings,
            missing,
        );
        match outcome {
            Some("not-provided") => assert_invalid(&decided, "not-provided", &mut store, id),
            Some(outcome) => assert_eq!(decided.outcome, outcome, "{id}: {decided:?}"),
            None => assert!(
                !matches!(
                    decided.outcome.as_str(),
                    "not-provided" | "missing-provided"
                ),
                "{id}: {decided:?}"
            ),
        }
    }
}

/// A consistent refusal whose external cause does not hold changes nothing.
#[test]
fn a_refusal_the_cause_does_not_confirm_is_unconfirmed() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");
    let mut decider = Never::default();
    let decided = decide(
        &ir,
        &mut store,
        &mut decider,
        "d1",
        json!(["channel"]),
        json!([]),
        json!(["channel"]),
    );
    assert_invalid(&decided, "unconfirmed", &mut store, "d1");
    assert_eq!(decider.asked, [REFUSED]);
}

/// An offer names a declared environment: the stored reference `DecideAdmission` reads is never
/// dangling.
#[test]
fn an_offer_to_an_undeclared_environment_is_refused() {
    let ir = specification();
    let mut store = MemoryStore::default();
    let offered = offer_to(&ir, &mut store, "d1", "nowhere");
    assert_eq!(offered.outcome, "unknown-environment", "{offered:?}");
    assert_eq!(
        offered.error.as_ref().map(|error| error.name.as_str()),
        Some("spindle.deployment.EnvironmentUnknown")
    );
    assert!(offered.events.is_empty(), "{offered:?}");
    assert_eq!(store.load(DEPLOYMENT, &json!("d1")).unwrap(), None);
}

/// Declares `environment` in the model as the runtime reads it for `requirements`: each required
/// capability with every implementation the environment document lists for it (a `prefix.*` key
/// read through [`spindle_capability::Environment::implementations`]).
fn declare_for(
    ir: &EssIr,
    store: &mut dyn Store,
    environment: &spindle_capability::Environment,
    requirements: &spindle_capability::Requirements,
) {
    let provides: Vec<Value> = requirements
        .keys()
        .flat_map(|capability| {
            environment
                .implementations(capability.as_str())
                .unwrap_or_default()
                .iter()
                .map(move |implementation| {
                    json!({"capability": capability.as_str(), "implementation": implementation})
                })
        })
        .collect();
    declare(ir, store, &environment.name, Value::Array(provides));
}

/// Derives what `bundle` requires, admits it against `environment`, and declares, offers and
/// decides it in the model as the runtime would; returns the decision and the bindings admission
/// chose.
fn admit_through_the_model(
    ir: &EssIr,
    store: &mut dyn Store,
    id: &str,
    bundle: &EssIr,
    environment: &spindle_capability::Environment,
) -> (Executed, Value) {
    let requirements = spindle_capability::derive(bundle, None).expect("the whole bundle");
    let admission = spindle_capability::admit(&requirements, environment);
    let bindings = bindings_of(&admission);
    declare_for(ir, store, environment, &requirements);
    let offered = offer_to(ir, store, id, &environment.name);
    assert_eq!(offered.outcome, "offered", "{offered:?}");
    let decided = decide(
        ir,
        store,
        &mut Never::default(),
        id,
        json!(names(&requirements)),
        bindings.clone(),
        json!([]),
    );
    (decided, bindings)
}

#[test]
fn preference_order_is_declared() {
    let ir = specification();
    let laptop = std::fs::read_to_string(workspace().join("examples/environments/laptop.toml"))
        .expect("the environment reads");
    for (order, preferred) in [
        ("[\"sqlite\", \"memory\"]", "sqlite"),
        ("[\"memory\", \"sqlite\"]", "memory"),
    ] {
        let text = laptop.replace(
            "\"store.durable\" = [\"sqlite\", \"memory\"]",
            &format!("\"store.durable\" = {order}"),
        );
        assert!(text.contains(&format!("\"store.durable\" = {order}")));
        let environment =
            spindle_capability::Environment::parse(&text).expect("the environment parses");
        let mut store = MemoryStore::default();
        let (decided, bindings) =
            admit_through_the_model(&ir, &mut store, preferred, &ir, &environment);
        assert_eq!(decided.outcome, "admitted", "{decided:?}");
        let durable = bindings
            .as_array()
            .expect("a list")
            .iter()
            .find(|binding| binding["capability"] == "store.durable")
            .expect("store.durable is bound");
        assert_eq!(durable["implementation"], preferred, "{order}");
    }
}

#[test]
fn requires_is_tied_to_the_bundle() {
    let ir = specification();
    let requirements = spindle_capability::derive(&ir, None).expect("the whole bundle");
    let laptop = environment("laptop.toml");
    let mut store = MemoryStore::default();
    let (decided, bindings) = admit_through_the_model(&ir, &mut store, "d1", &ir, &laptop);
    assert_eq!(decided.outcome, "admitted", "{decided:?}");
    let stored = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(
        stored.row.fields.get("requires"),
        Some(&json!(names(&requirements)))
    );

    let mut short = bindings.as_array().expect("a list").clone();
    short.pop();
    let offered = offer_to(&ir, &mut store, "d2", "laptop");
    assert_eq!(offered.outcome, "offered", "{offered:?}");
    let decided = decide(
        &ir,
        &mut store,
        &mut Never::default(),
        "d2",
        json!(names(&requirements)),
        Value::Array(short),
        json!([]),
    );
    assert_invalid(&decided, "not-decided", &mut store, "d2");
}

#[test]
fn one_bundle_in_three_environments_is_decided() {
    let ir = specification();
    let rally = spindle_spec::load(&workspace().join("examples/rally/ess"))
        .expect("rally compiles")
        .ir;
    let microvm = environment("microvm.toml");
    let mut store = MemoryStore::default();
    let (decided, bindings) = admit_through_the_model(&ir, &mut store, "rally", &rally, &microvm);
    assert_eq!(decided.outcome, "admitted", "{decided:?}");
    let bound = bindings.as_array().expect("a list");

    for component in ["rally-table", "rally-player-west", "rally-player-east"] {
        let requirements =
            spindle_capability::derive(&rally, Some(component)).expect("the component exists");
        let admission = spindle_capability::admit(&requirements, &microvm);
        for binding in bindings_of(&admission).as_array().expect("a list") {
            assert!(
                bound.contains(binding),
                "{component}'s {binding} is a binding of the one deployment: {bindings}"
            );
        }
    }
}

#[test]
fn stop_from_admitted_and_offered_is_stated() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "admitted");
    admit(&ir, &mut store, &mut Never::default(), "admitted");
    let stopped = run(
        &ir,
        &mut store,
        &mut Never::default(),
        "spindle.deployment.StopDeployment",
        json!({"deployment_id": "admitted"}),
    );
    assert_eq!(stopped.outcome, "stopped", "{stopped:?}");
    assert_eq!(state_of(&mut store, "admitted"), "Stopped");

    offer_to(&ir, &mut store, "offered", "laptop");
    let before = store.load(DEPLOYMENT, &json!("offered")).unwrap().unwrap();
    let ignored = run(
        &ir,
        &mut store,
        &mut Never::default(),
        "spindle.deployment.StopDeployment",
        json!({"deployment_id": "offered"}),
    );
    assert_eq!(ignored.outcome, "already-ended", "{ignored:?}");
    assert_eq!(ignored.error, None);
    assert!(ignored.events.is_empty(), "{ignored:?}");
    assert_eq!(
        store.load(DEPLOYMENT, &json!("offered")).unwrap().unwrap(),
        before
    );
    let admitted = admit(&ir, &mut store, &mut Never::default(), "offered");
    assert_eq!(
        admitted.outcome, "admitted",
        "a stop does not withdraw an offer: {admitted:?}"
    );
}
