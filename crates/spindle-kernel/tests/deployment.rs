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
    assert_eq!(decider.asked, vec![REFUSED.to_owned()]);
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
        json!({"deployment_id": "d1", "bindings": [], "missing": ["channel"]}),
    );
    assert_eq!(refused.outcome, "refused");
    assert_eq!(
        refused.events[0].name,
        "spindle.deployment.DeploymentRefused"
    );
    let stored = store.load(DEPLOYMENT, &json!("d1")).unwrap().unwrap();
    assert_eq!(stored.row.state, "Refused");
    assert_eq!(stored.row.fields.get("missing"), Some(&json!(["channel"])));
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
    // As in ESS, `wrong_state:` is not read as a guard: the external cause is asked, and the
    // branch it selects does not move from `Admitted`, which is the wrong-state answer.
    assert_eq!(
        decider.asked,
        ["external.spindle.deployment.DecideAdmission.refused"]
    );
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
