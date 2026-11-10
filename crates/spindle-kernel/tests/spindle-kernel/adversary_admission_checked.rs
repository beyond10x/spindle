//! Adversary cases against `spindle.deployment.DecideAdmission` (story
//! `admission-decision-checked-by-model`), run through the kernel: the decision is checked by the
//! model against the deployment's environment, not by its caller.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

const DEPLOYMENT: &str = "spindle.deployment.Deployment";
const DECIDE: &str = "spindle.deployment.DecideAdmission";

fn specification() -> EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ess");
    spindle_spec::load(&root).expect("ess/ compiles").ir
}

/// Answers every external question `true`: the admitter's caller confirms whatever it is asked.
struct Yes;

impl Decider for Yes {
    fn decide(&mut self, _: &External<'_>) -> Result<bool, String> {
        Ok(true)
    }
}

fn run(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value) -> Executed {
    let Value::Object(map) = request else {
        panic!("an input is an object");
    };
    let map: Map<String, Value> = map;
    execute(ir, store, &mut Yes, command, &map)
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn declared_and_offered(ir: &EssIr, store: &mut dyn Store, id: &str, provides: &Value) {
    let declared = run(
        ir,
        store,
        "spindle.deployment.DeclareEnvironment",
        json!({
            "environment_id": id,
            "os": "linux",
            "arch": "x86_64",
            "provides": crate::provides::provisions(provides),
        }),
    );
    assert_eq!(declared.outcome, "declared", "{declared:?}");
    let offered = run(
        ir,
        store,
        "spindle.deployment.OfferBundle",
        json!({"deployment_id": id, "bundle": "sha256:0f", "environment_id": id}),
    );
    assert_eq!(offered.outcome, "offered", "{offered:?}");
}

fn state_of(store: &mut dyn Store, id: &str) -> String {
    store
        .load(DEPLOYMENT, &json!(id))
        .expect("the store reads")
        .expect("the deployment exists")
        .row
        .state
}

/// The model reads the deployment's environment for `not-provided`, and the story's outcome is
/// that it "decides what it can decide from its own rows". `laptop` provides `store.durable` by
/// `sqlite`, so a refusal naming `store.durable` missing contradicts the environment's row, and
/// `spindle_capability::admit` never builds one: it names a capability missing only where the
/// environment lists no implementation for it. The model does not read `provides` for `missing`
/// (`refused`'s summary), so a caller that confirms the external cause refuses the deployment.
#[test]
fn adversary_a_refusal_of_a_capability_the_environment_provides_is_not_refused() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declared_and_offered(
        &ir,
        &mut store,
        "laptop",
        &json!([{"capability": "store.durable", "implementation": "sqlite"}]),
    );
    let decided = run(
        &ir,
        &mut store,
        DECIDE,
        json!({
            "deployment_id": "laptop",
            "requires": ["store.durable"],
            "bindings": [],
            "missing": ["store.durable"],
        }),
    );
    assert_ne!(
        decided.outcome, "refused",
        "`laptop` provides `store.durable`, and the model refused it as missing: {decided:?}"
    );
    assert_eq!(state_of(&mut store, "laptop"), "Offered");
}

/// A deterministic generator, so a failing decision is reproducible from its index.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn subset<T: Clone>(&mut self, of: &[T], most: u64) -> Vec<T> {
        (0..self.below(most + 1))
            .map(|_| of[self.below(of.len() as u64) as usize].clone())
            .collect()
    }
}

/// Whether `decision` is valid by `AdmissionDecisionInvalid`'s own text, the refusal clause read as
/// the external cause confirming it: bindings distinct by capability, each required and provided
/// for that capability by `provides`; each missing one required, not bound and not a capability
/// `provides` lists (correction round 1, `missing-provided`); each required one bound or missing.
fn valid(
    requires: &[&str],
    bindings: &[(&str, &str)],
    missing: &[&str],
    provides: &[(&str, &str)],
) -> bool {
    let distinct = bindings
        .iter()
        .enumerate()
        .all(|(i, (c, _))| bindings[i + 1..].iter().all(|(d, _)| c != d));
    distinct
        && bindings
            .iter()
            .all(|binding| requires.contains(&binding.0) && provides.contains(binding))
        && missing.iter().all(|m| {
            requires.contains(m)
                && bindings.iter().all(|(c, _)| c != m)
                && provides.iter().all(|(c, _)| c != m)
        })
        && requires
            .iter()
            .all(|c| missing.contains(c) || bindings.iter().any(|(b, _)| b == c))
}

/// Property, fixed seed: over decisions drawn from a small alphabet, so collisions are common, an
/// offered deployment is answered `admitted` only for a valid decision naming nothing missing, and
/// `refused` only for a valid decision naming something missing, whatever the external cause says.
#[test]
fn adversary_admitted_and_refused_answer_only_a_valid_decision() {
    let ir = specification();
    let capabilities = ["clock", "store.durable", "channel"];
    let implementations = ["system", "sqlite", "tcp"];
    let pairs: Vec<(&str, &str)> = capabilities
        .iter()
        .flat_map(|c| implementations.iter().map(move |i| (*c, *i)))
        .collect();
    let mut random = Lcg(0x5EED_0017);
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for index in 0..400 {
        let provides = random.subset(&pairs, 4);
        let requires = random.subset(&capabilities, 3);
        let bindings = random.subset(&pairs, 3);
        let missing = random.subset(&capabilities, 2);
        let id = format!("d{index}");
        let mut store = MemoryStore::default();
        let provided: Vec<Value> = provides
            .iter()
            .map(|(c, i)| json!({"capability": c, "implementation": i}))
            .collect();
        declared_and_offered(&ir, &mut store, &id, &Value::Array(provided));
        let decided = run(
            &ir,
            &mut store,
            DECIDE,
            json!({
                "deployment_id": id,
                "requires": requires,
                "bindings": bindings
                    .iter()
                    .map(|(c, i)| json!({"capability": c, "implementation": i}))
                    .collect::<Vec<_>>(),
                "missing": missing,
            }),
        );
        let is_valid = valid(&requires, &bindings, &missing, &provides);
        let case = format!(
            "#{index}: provides {provides:?}, requires {requires:?}, bindings {bindings:?}, \
             missing {missing:?} -> {}",
            decided.outcome
        );
        match decided.outcome.as_str() {
            "admitted" => assert!(is_valid && missing.is_empty(), "{case}"),
            "refused" => assert!(is_valid && !missing.is_empty(), "{case}"),
            _ => {
                assert!(decided.error.is_some(), "{case}");
                assert_eq!(state_of(&mut store, &id), "Offered", "{case}");
                assert!(!is_valid, "a valid decision was turned away: {case}");
            }
        }
        *counts.entry(decided.outcome.clone()).or_default() += 1;
    }
    for outcome in [
        "admitted",
        "refused",
        "bound-twice",
        "not-decided",
        "not-provided",
        "missing-provided",
    ] {
        assert!(
            counts.get(outcome).copied().unwrap_or(0) > 0,
            "the generator never reached `{outcome}`: {counts:?}"
        );
    }
}
