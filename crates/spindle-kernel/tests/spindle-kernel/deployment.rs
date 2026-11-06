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

fn offer(ir: &EssIr, store: &mut dyn Store, id: &str) -> Executed {
    run(
        ir,
        store,
        &mut Never::default(),
        "spindle.deployment.OfferBundle",
        json!({
            "deployment_id": id,
            "bundle": "sha256:0f",
            "environment_id": "laptop",
            "requires": ["store.durable"],
        }),
    )
}

fn admit(ir: &EssIr, store: &mut dyn Store, decider: &mut dyn Decider, id: &str) -> Executed {
    run(
        ir,
        store,
        decider,
        "spindle.deployment.DecideAdmission",
        json!({
            "deployment_id": id,
            "bindings": [{"capability": "store.durable", "implementation": "sqlite"}],
            "missing": [],
        }),
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
    assert_eq!(stored.row.fields.get("bundle"), Some(&json!("sha256:0f")));
}

#[test]
fn an_external_cause_that_holds_refuses_the_deployment() {
    let ir = specification();
    let mut store = MemoryStore::default();
    offer(&ir, &mut store, "d1");

    let mut lacking =
        |question: &External<'_>| -> Result<bool, String> { Ok(question.capability == REFUSED) };
    let refused = run(
        &ir,
        &mut store,
        &mut lacking,
        "spindle.deployment.DecideAdmission",
        json!({"deployment_id": "d1", "bindings": [], "missing": ["store.durable"]}),
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
    // `admitted` is selected by its guard over the held row before any external branch, and it
    // does not move from `Admitted`, which is the wrong-state answer; `refused` is not eligible
    // with nothing missing, so its cause is never asked.
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

fn offer_requiring(
    ir: &EssIr,
    store: &mut dyn Store,
    id: &str,
    environment: &str,
    requires: Value,
) -> Executed {
    let offered = run(
        ir,
        store,
        &mut Never::default(),
        "spindle.deployment.OfferBundle",
        json!({
            "deployment_id": id,
            "bundle": "sha256:0f",
            "environment_id": environment,
            "requires": requires,
        }),
    );
    assert_eq!(offered.outcome, "offered", "{offered:?}");
    offered
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
        "spindle.deployment.DecideAdmission",
        json!({"deployment_id": id, "bindings": bindings, "missing": missing}),
    )
}

/// `executed` is the invalid-decision refusal and left the deployment as it was.
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
        ("unbound", json!([durable]), "not-decided"),
        (
            "twice",
            json!([durable, {"capability": "store.durable", "implementation": "memory"}, clock]),
            "bound-twice",
        ),
        (
            "unrequired",
            json!([durable, clock, {"capability": "channel", "implementation": "tcp"}]),
            "not-decided",
        ),
        (
            "admitted-but-missing",
            json!([durable, clock]),
            "not-decided",
        ),
    ];
    for (id, bindings, outcome) in cases {
        let mut store = MemoryStore::default();
        offer_requiring(&ir, &mut store, id, "laptop", required.clone());
        let missing = if id == "admitted-but-missing" {
            json!(["clock"])
        } else {
            json!([])
        };
        let decided = decide(
            &ir,
            &mut store,
            &mut Never::default(),
            id,
            bindings,
            missing,
        );
        assert_invalid(&decided, outcome, &mut store, id);
    }

    let mut store = MemoryStore::default();
    offer_requiring(&ir, &mut store, "exact", "laptop", required);
    let admitted = decide(
        &ir,
        &mut store,
        &mut Never::default(),
        "exact",
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
    offer_requiring(
        &ir,
        &mut store,
        "d1",
        "laptop",
        json!(["clock", "store.durable"]),
    );
    let mut lacking =
        |question: &External<'_>| -> Result<bool, String> { Ok(question.capability == REFUSED) };
    let refused = decide(
        &ir,
        &mut store,
        &mut lacking,
        "d1",
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
    offer_requiring(&ir, &mut store, "d1", "laptop", json!(["store.durable"]));
    let mut refusing = Always::default();
    let decided = decide(&ir, &mut store, &mut refusing, "d1", json!([]), json!([]));
    assert_invalid(&decided, "not-decided", &mut store, "d1");
    assert!(
        refusing.asked.is_empty(),
        "a refusal naming nothing is not eligible, so its cause is never asked: {:?}",
        refusing.asked
    );
}

/// Derives what `bundle` requires, admits it against `environment`, and offers and decides it in
/// the model as the runtime would; returns the decision and the bindings admission chose.
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
    offer_requiring(
        ir,
        store,
        id,
        &environment.name,
        json!(names(&requirements)),
    );
    let decided = decide(
        ir,
        store,
        &mut Never::default(),
        id,
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
    let mut store = MemoryStore::default();
    let (decided, bindings) =
        admit_through_the_model(&ir, &mut store, "d1", &ir, &environment("laptop.toml"));
    assert_eq!(decided.outcome, "admitted", "{decided:?}");
    let stored = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(
        stored.row.fields.get("requires"),
        Some(&json!(names(&requirements)))
    );

    let mut short = bindings.as_array().expect("a list").clone();
    short.pop();
    let mut store = MemoryStore::default();
    offer_requiring(&ir, &mut store, "d2", "laptop", json!(names(&requirements)));
    let decided = decide(
        &ir,
        &mut store,
        &mut Never::default(),
        "d2",
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

    offer(&ir, &mut store, "offered");
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
