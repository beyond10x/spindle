//! Adversary cases for minted `String`-newtype identities.
//!
//! The fixture `tests/fixtures/adversary_mint_text` (valid under `ess specify validate`) holds
//! three identities: `LongId`, a prefixed text whose one invariant (`value.count <= 64`) every
//! minted text meets; `NestedId`, whose inner newtype's prefix `nb_` extends the outer `n`, as
//! ESS allows (`ess_domain::types`, `validate_prefix`: nested prefixes extend one another); and
//! `PinnedId`, a `Uuid` newtype whose invariant a fresh UUID does not meet.

use std::collections::BTreeMap;
use std::path::Path;

use ess_domain::entity::StateName;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Identity, MemoryStore, NoExternals, Store, Uuid, UuidV4, execute, execute_minting,
};

fn load(fixture: &str) -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    spindle_spec::load(&root)
        .unwrap_or_else(|error| panic!("{fixture} compiles: {error:?}"))
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

/// Whether ESS's own conformance validator admits `identity` as the identity of a stored
/// `entity` holding `fields`, in `state`.
fn ess_admits(
    ir: &spindle_spec::EssIr,
    entity: &str,
    identity: &str,
    fields: &[(&str, &str)],
    state: &str,
) -> Result<(), String> {
    let reference = serde_json::from_value(json!(entity)).expect("an entity ref");
    let values = fields
        .iter()
        .map(|(name, value)| ((*name).to_owned(), Node::Text((*value).to_owned())))
        .collect::<BTreeMap<_, _>>();
    ess_conformance::input::validate_entity_setup(
        ir,
        &reference,
        &Node::Text(identity.to_owned()),
        &values,
        &StateName::new(state).expect("a state"),
    )
}

fn opened(executed: &spindle_kernel::Executed, field: &str) -> String {
    executed.events[0].payload[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is text in {:?}", executed.events[0].payload))
        .to_owned()
}

#[test]
fn the_minted_badge_identity_is_a_value_of_its_newtype_to_ess() {
    let ir = load("mint_text");
    let mut store = MemoryStore::default();
    let issued = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "minttext.badge.IssueBadge",
        &input(json!({"holder": "ada"})),
    )
    .expect("the badge is issued");
    let identity = opened(&issued, "badge_id");
    ess_admits(
        &ir,
        "minttext.badge.Badge",
        &identity,
        &[("holder", "ada")],
        "Active",
    )
    .unwrap_or_else(|error| panic!("ESS refuses the minted {identity}: {error}"));
}

#[test]
fn a_repeated_text_mint_is_refused_and_the_first_instance_is_kept() {
    let ir = load("mint_text");
    let mut store = MemoryStore::default();
    let fixed = Uuid::parse_str("5e0c9a4b-1f2d-4c3e-9a8b-7d6c5b4a3f21").expect("a UUID");
    let mut mint = |_: &Identity<'_>| fixed;
    let first = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "minttext.badge.IssueBadge",
        &input(json!({"holder": "ada"})),
    )
    .expect("the first badge is issued");
    let identity = opened(&first, "badge_id");
    assert_eq!(identity, format!("bdg_{fixed}"));
    let second = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "minttext.badge.IssueBadge",
        &input(json!({"holder": "grace"})),
    );
    assert!(
        second.is_err(),
        "a second creation under a held identity is refused: {second:?}"
    );
    let stored = store
        .load("minttext.badge.Badge", &json!(identity))
        .expect("the store reads")
        .expect("the first badge is kept");
    assert_eq!(stored.row.fields.get("holder"), Some(&json!("ada")));
}

#[test]
fn many_creations_mint_distinct_text_identities() {
    let ir = load("mint_text");
    let mut store = MemoryStore::default();
    let mut mint = UuidV4;
    let mut seen = std::collections::BTreeSet::new();
    for round in 0..200 {
        let issued = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            "minttext.badge.IssueBadge",
            &input(json!({"holder": format!("holder-{round}")})),
        )
        .expect("each badge is issued");
        assert!(seen.insert(opened(&issued, "badge_id")), "round {round}");
    }
}

#[test]
fn a_text_identity_whose_length_invariant_every_minted_text_meets_is_minted() {
    let ir = load("adversary_mint_text");
    let mut store = MemoryStore::default();
    let executed = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "advmint.tag.OpenLong",
        &input(json!({"note": "kept"})),
    )
    .expect(
        "the story's outcome: a minted identity satisfies the newtype's prefix, alphabet and \
         length invariants; `lng_` and a UUID's text are 40 characters, inside `value.count <= 64`",
    );
    let identity = opened(&executed, "long_id");
    assert!(identity.starts_with("lng_"), "{identity}");
    ess_admits(
        &ir,
        "advmint.tag.Long",
        &identity,
        &[("note", "kept")],
        "Kept",
    )
    .unwrap_or_else(|error| panic!("ESS refuses the minted {identity}: {error}"));
}

#[test]
fn a_text_identity_whose_inner_prefix_extends_the_outer_one_is_minted_under_the_longer() {
    let ir = load("adversary_mint_text");
    let mut store = MemoryStore::default();
    let executed = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "advmint.tag.OpenNested",
        &input(json!({"note": "kept"})),
    )
    .expect("ESS admits `nb_` under `n`, and `nb_` and a UUID's text satisfy both layers");
    let identity = opened(&executed, "nested_id");
    assert!(identity.starts_with("nb_"), "{identity}");
    ess_admits(
        &ir,
        "advmint.tag.Nested",
        &identity,
        &[("note", "kept")],
        "Kept",
    )
    .unwrap_or_else(|error| panic!("ESS refuses the minted {identity}: {error}"));
}

#[test]
fn a_uuid_identity_whose_invariant_a_fresh_uuid_misses_is_refused_by_name_before_minting() {
    let ir = load("adversary_mint_text");
    let mut store = MemoryStore::default();
    let mut asked = 0_u32;
    let mut mint = |_: &Identity<'_>| {
        asked += 1;
        Uuid::parse_str("5e0c9a4b-1f2d-4c3e-9a8b-7d6c5b4a3f21").expect("a UUID")
    };
    let error = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "advmint.tag.OpenPinned",
        &input(json!({"note": "kept"})),
    )
    .expect_err("no fresh UUID is a value of PinnedId");
    match &error {
        ExecuteError::Unsupported { construct } => {
            assert!(construct.contains("advmint.tag.PinnedId"), "{construct}");
            assert!(construct.contains("pinned_id"), "{construct}");
        }
        other => panic!(
            "a newtype the runtime cannot satisfy is refused as Unsupported, naming the type, as \
             the text path refuses `SealId`; got {other:?}"
        ),
    }
    assert_eq!(asked, 0, "the mint is not asked for a value it cannot give");
}

/// `Short`'s entity invariant `short_id.count <= 10` holds for no `s_` and UUID text (38
/// characters), so the creation can never commit. The claim is that such a command is refused
/// by name before it runs, not refused as a broken invariant after the mint.
#[test]
fn a_text_identity_the_entity_invariant_bounds_below_a_minted_text_is_refused_by_name() {
    let ir = load("adversary_mint_text");
    let mut store = MemoryStore::default();
    let mut asked = 0_u32;
    let mut mint = |_: &Identity<'_>| {
        asked += 1;
        Uuid::parse_str("5e0c9a4b-1f2d-4c3e-9a8b-7d6c5b4a3f21").expect("a UUID")
    };
    let error = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "advmint.tag.OpenShort",
        &input(json!({"note": "kept"})),
    )
    .expect_err("no minted text fits ten characters");
    match &error {
        ExecuteError::Unsupported { construct } => {
            assert!(construct.contains("short_id"), "{construct}");
        }
        other => panic!("expected Unsupported naming short_id, got {other:?}"),
    }
    assert_eq!(
        asked, 0,
        "the mint is not asked for a value no instance can hold"
    );
}
