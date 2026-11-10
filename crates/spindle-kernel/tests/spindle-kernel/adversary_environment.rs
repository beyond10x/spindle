//! Adversary cases against `spindle.environment.Provision` (story
//! `environment-implementations-specified`), run through the kernel: `DeclareEnvironment`'s
//! `key-twice` and `no-implementation`, and `DecideAdmission`'s `not-provided` against the
//! runtime's rule, `spindle_capability::Environment::implementations`.
//!
//! `ADVERSARY_ESS_ROOT` points the cases at another copy of `ess/`, to run them against a mutant.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

const ENVIRONMENT: &str = "spindle.deployment.Environment";
const PROVIDES_INVALID: &str = "spindle.environment.ProvidesInvalid";

fn specification() -> EssIr {
    let root = std::env::var_os("ADVERSARY_ESS_ROOT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ess"),
        PathBuf::from,
    );
    spindle_spec::load(&root).expect("ess/ compiles").ir
}

/// Answers every external question `true`.
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

fn declare(ir: &EssIr, store: &mut dyn Store, id: &str, provides: Value) -> Executed {
    run(
        ir,
        store,
        "spindle.deployment.DeclareEnvironment",
        json!({"environment_id": id, "os": "linux", "arch": "x86_64", "provides": provides}),
    )
}

/// The runtime's document, `key = [implementations]`, as the model's `List<Provision>`, one
/// Provision per key exactly as written.
fn provisions(document: &BTreeMap<String, Vec<String>>) -> Value {
    document
        .iter()
        .map(
            |(key, implementations)| json!({"capability": key, "implementations": implementations}),
        )
        .collect()
}

fn runtime(document: &BTreeMap<String, Vec<String>>) -> spindle_capability::Environment {
    spindle_capability::Environment {
        format: spindle_capability::ENVIRONMENT_FORMAT.to_owned(),
        name: "edge".to_owned(),
        os: "linux".to_owned(),
        arch: "x86_64".to_owned(),
        provides: document.clone(),
    }
}

fn document(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    pairs
        .iter()
        .map(|(key, implementations)| {
            (
                (*key).to_owned(),
                implementations.iter().map(|i| (*i).to_owned()).collect(),
            )
        })
        .collect()
}

/// Declares `document` as `edge`, offers `id` to it and decides it.
fn decide(
    ir: &EssIr,
    document: &BTreeMap<String, Vec<String>>,
    id: &str,
    requires: &[&str],
    bindings: &[(&str, &str)],
    missing: &[&str],
) -> Executed {
    let mut store = MemoryStore::default();
    let declared = declare(ir, &mut store, "edge", provisions(document));
    assert_eq!(declared.outcome, "declared", "{declared:?}");
    let offered = run(
        ir,
        &mut store,
        "spindle.deployment.OfferBundle",
        json!({"deployment_id": id, "bundle": "sha256:0f", "environment_id": "edge"}),
    );
    assert_eq!(offered.outcome, "offered", "{offered:?}");
    let bindings: Vec<Value> = bindings
        .iter()
        .map(|(c, i)| json!({"capability": c, "implementation": i}))
        .collect();
    run(
        ir,
        &mut store,
        "spindle.deployment.DecideAdmission",
        json!({"deployment_id": id, "requires": requires, "bindings": bindings, "missing": missing}),
    )
}

fn assert_provides_invalid(executed: &Executed, outcome: &str, store: &mut dyn Store, id: &str) {
    assert_eq!(executed.outcome, outcome, "{executed:?}");
    assert_eq!(
        executed.error.as_ref().map(|error| error.name.as_str()),
        Some(PROVIDES_INVALID),
        "{executed:?}"
    );
    assert!(executed.events.is_empty(), "{executed:?}");
    assert!(
        store.load(ENVIRONMENT, &json!(id)).unwrap().is_none(),
        "{outcome} recorded nothing"
    );
}

/// `key-twice`: one key in two Provisions, the same implementations or others, prefix or not.
#[test]
fn adversary_a_key_listed_twice_is_key_twice() {
    let ir = specification();
    for (id, provides) in [
        (
            "same-key-other-implementations",
            json!([
                {"capability": "store.durable", "implementations": ["sqlite"]},
                {"capability": "store.durable", "implementations": ["memory"]},
            ]),
        ),
        (
            "same-prefix-key",
            json!([
                {"capability": "store.*", "implementations": ["memory"]},
                {"capability": "clock", "implementations": ["system"]},
                {"capability": "store.*", "implementations": ["memory"]},
            ]),
        ),
    ] {
        let mut store = MemoryStore::default();
        let executed = declare(&ir, &mut store, id, provides);
        assert_provides_invalid(&executed, "key-twice", &mut store, id);
    }
}

/// `no-implementation`: a key, first, last or alone, with an empty list.
#[test]
fn adversary_a_key_with_no_implementation_is_no_implementation() {
    let ir = specification();
    for (id, provides) in [
        (
            "alone",
            json!([{"capability": "store.durable", "implementations": []}]),
        ),
        (
            "last-prefix",
            json!([
                {"capability": "store.durable", "implementations": ["sqlite"]},
                {"capability": "external.*", "implementations": []},
            ]),
        ),
    ] {
        let mut store = MemoryStore::default();
        let executed = declare(&ir, &mut store, id, provides);
        assert_provides_invalid(&executed, "no-implementation", &mut store, id);
    }
}

/// Keys the runtime's parser accepts are declared: none at all, a prefix beside an exact key
/// under it, `*` alone, `.*`, a trailing dot, the empty key.
#[test]
fn adversary_keys_the_document_parser_accepts_are_declared() {
    let ir = specification();
    for (id, pairs) in [
        ("empty", &[][..]),
        (
            "prefix-beside-exact",
            &[
                ("store.*", &["memory"][..]),
                ("store.durable", &["sqlite"][..]),
            ][..],
        ),
        ("star", &[("*", &["x"][..])][..]),
        ("dot-star", &[(".*", &["x"][..])][..]),
        ("trailing-dot", &[("store.", &["x"][..])][..]),
        ("empty-key", &[("", &["x"][..])][..]),
    ] {
        let document = document(pairs);
        let text = format!(
            "format = \"spindle-environment/1\"\nname = \"e\"\nos = \"linux\"\narch = \"x86_64\"\n[provides]\n{}",
            document
                .iter()
                .map(|(k, v)| format!("{k:?} = {v:?}\n"))
                .collect::<String>()
        );
        spindle_capability::Environment::parse(&text)
            .unwrap_or_else(|error| panic!("{id}: the runtime parses it: {error}"));
        let mut store = MemoryStore::default();
        let executed = declare(&ir, &mut store, id, provisions(&document));
        assert_eq!(executed.outcome, "declared", "{id}: {executed:?}");
    }
}

/// A key the runtime does not read as a prefix (`*`, `store*`, `store.`, `store.**`) provides only
/// itself: a binding of `store.durable` through it is `not-provided`, as the runtime finds none.
#[test]
fn adversary_a_key_that_is_not_a_prefix_does_not_disable_not_provided() {
    let ir = specification();
    for key in ["*", "store*", "store.", "store.**"] {
        let document = document(&[(key, &["x"][..])]);
        assert_eq!(
            runtime(&document).implementations("store.durable"),
            None,
            "{key}"
        );
        let decided = decide(
            &ir,
            &document,
            "d",
            &["store.durable"],
            &[("store.durable", "x")],
            &[],
        );
        assert_eq!(decided.outcome, "not-provided", "{key}: {decided:?}");
    }
}

/// story:environment-implementations-specified (correction round 1, renamed from
/// `adversary_a_binding_no_key_covers_is_not_admitted_beside_a_prefix_key`): `admitted`'s
/// summary says that once any `prefix.*` key is present, a binding whose capability has no exact
/// key is left to the runtime. `laptop.toml` as written holds `external.*`; it provides nothing
/// for `net.http`, the runtime finds no implementation for it, and the model admits it. This is
/// the limit `ess/domains/deployment.yaml:291` marks `UNMAPPED:` (a prefix match against a list
/// element is `unparsable_predicate` in ESS 0.90). Without the prefix key the same binding is
/// `not-provided`, so the limit is exactly the prefix key's presence.
#[test]
fn adversary_a_binding_no_key_covers_beside_a_prefix_key_is_left_to_the_runtime() {
    let ir = specification();
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let laptop = spindle_capability::Environment::parse(
        &std::fs::read_to_string(&path).expect("laptop.toml reads"),
    )
    .expect("laptop.toml parses");
    for (capability, implementation) in [("net.http", "tcp"), ("external", "operator-prompt")] {
        assert_eq!(
            laptop.implementations(capability),
            None,
            "the runtime finds no implementation of {capability}"
        );
        let decided = decide(
            &ir,
            &laptop.provides,
            "d",
            &[capability],
            &[(capability, implementation)],
            &[],
        );
        assert_eq!(
            decided.outcome, "admitted",
            "beside `external.*` the model leaves `{capability}` to the runtime \
             (deployment.yaml:291 UNMAPPED): {decided:?}"
        );
        let mut without_prefix = laptop.provides.clone();
        assert!(without_prefix.remove("external.*").is_some());
        let decided = decide(
            &ir,
            &without_prefix,
            "d",
            &[capability],
            &[(capability, implementation)],
            &[],
        );
        assert_eq!(
            decided.outcome, "not-provided",
            "without a prefix key `{capability}` bound to `{implementation}` is checked: \
             {decided:?}"
        );
    }
}

/// A small fixed-seed generator, so the property below needs no dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
}

/// Property: the model never refuses as `not-provided` or `missing-provided` what the runtime's
/// rule provides, and the runtime's own decision is always admitted or refused.
#[test]
fn adversary_the_model_never_refuses_what_the_runtime_provides() {
    let ir = specification();
    let keys = [
        "store.durable",
        "store.*",
        "store.durable.*",
        "external.*",
        "*",
        ".*",
        "store.",
        "clock",
    ];
    let capabilities = [
        "store.durable",
        "store.durable.x",
        "store.events",
        "external.a.b",
        "clock",
        "channel",
    ];
    let implementations = ["a", "b", "c"];
    let mut random = Lcg(0x5eed_0017);
    for round in 0..300 {
        let mut document = BTreeMap::new();
        for _ in 0..=(random.next() % 4) {
            let key = random.pick(&keys).to_owned();
            let list: Vec<String> = (0..=(random.next() % 2))
                .map(|_| random.pick(&implementations).to_owned())
                .collect();
            document.insert(key, list);
        }
        let mut requires: Vec<&str> = Vec::new();
        for _ in 0..=(random.next() % 3) {
            let capability = random.pick(&capabilities);
            if !requires.contains(&capability) {
                requires.push(capability);
            }
        }
        let environment = runtime(&document);
        let id = format!("r{round}");

        // The runtime's honest decision.
        let mut bindings = Vec::new();
        let mut missing = Vec::new();
        for capability in &requires {
            match environment
                .implementations(capability)
                .and_then(<[String]>::first)
            {
                Some(first) => bindings.push((*capability, first.as_str())),
                None => missing.push(*capability),
            }
        }
        let decided = decide(&ir, &document, &id, &requires, &bindings, &missing);
        let expected = if missing.is_empty() {
            "admitted"
        } else {
            "refused"
        };
        assert_eq!(
            decided.outcome, expected,
            "{document:?} {requires:?}: {decided:?}"
        );

        // Any implementation the runtime lists, not only the first, is not `not-provided`.
        let mut bindings = Vec::new();
        let mut missing = Vec::new();
        for capability in &requires {
            match environment.implementations(capability) {
                Some(list) => bindings.push((*capability, list.last().unwrap().as_str())),
                None => missing.push(*capability),
            }
        }
        let decided = decide(&ir, &document, &id, &requires, &bindings, &missing);
        assert!(
            !matches!(
                decided.outcome.as_str(),
                "not-provided" | "missing-provided"
            ),
            "{document:?} {bindings:?} {missing:?}: {decided:?}"
        );
    }
}
