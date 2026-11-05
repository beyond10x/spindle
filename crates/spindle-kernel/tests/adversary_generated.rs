//! Adversary cases for `story:generated-field-values`, over the neutral fixture
//! `tests/fixtures/adversary_generated`: a lot opened under a minted identity, with generated
//! struct members, two published sequences, and fields of types the runtime cannot mint.

use std::path::Path;
use std::time::{Duration, SystemTime};

use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Executed, Identity, MemoryStore, NoExternals, Store, Uuid, execute_at,
    execute_minting,
};

const LOT: &str = "forge.lot.Lot";

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_generated");
    spindle_spec::load(&root)
        .expect("the adversary fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

/// A mint answering `00000000-0000-4000-8000-<n>` in turn.
struct Counting(u64);

impl spindle_kernel::Mint for Counting {
    fn uuid(&mut self, _: &Identity<'_>) -> Uuid {
        self.0 += 1;
        Uuid::parse_str(&format!("00000000-0000-4000-8000-{:012}", self.0)).expect("a UUID")
    }
}

fn run(
    store: &mut MemoryStore,
    mint: &mut Counting,
    command: &str,
    sent: Value,
) -> Result<Executed, ExecuteError> {
    execute_minting(&ir(), store, &mut NoExternals, mint, command, &input(sent))
}

fn event<'e>(executed: &'e Executed, name: &str) -> &'e Map<String, Value> {
    &executed
        .events
        .iter()
        .find(|event| event.name == name)
        .unwrap_or_else(|| panic!("{name} is emitted: {:?}", executed.events))
        .payload
}

fn opened_lot(store: &mut MemoryStore, mint: &mut Counting) -> Value {
    let opened =
        run(store, mint, "forge.lot.OpenLot", json!({"note": "n"})).expect("the lot opens");
    event(&opened, "forge.lot.LotOpened")["lot_id"].clone()
}

/// ESS's interpreter publishes the created identity only in the field `instance:` names on the
/// event `instance:` names (`interpret::execute::emit`, `identity` is `Some` only where `event ==
/// handle`). A struct member of the same name, `origin.lot_id: Integer`, is a `{generated:
/// true}` of its own and is minted from its own type.
#[test]
fn a_generated_struct_member_named_like_the_minted_identity_is_minted_from_its_own_type() {
    let mut store = MemoryStore::default();
    let mut mint = Counting(0);
    let traced = run(
        &mut store,
        &mut mint,
        "forge.lot.TraceLot",
        json!({"note": "n"}),
    )
    .expect("a lot with a generated origin member opens");
    let payload = event(&traced, "forge.lot.LotTraced");
    let identity = payload["lot_id"].clone();
    let member = &payload["origin"]["lot_id"];
    assert!(
        member.is_i64(),
        "`origin.lot_id` is an Integer, minted from its own type, not the identity {identity}: \
         {member}"
    );
}

/// The same rule on a second emitted event: `LotCounted.lot_id: Integer` is not the field
/// `instance:` names, so ESS mints an Integer for it.
#[test]
fn a_generated_field_named_like_the_identity_on_another_event_is_minted_from_its_own_type() {
    let mut store = MemoryStore::default();
    let mut mint = Counting(0);
    let opened = run(
        &mut store,
        &mut mint,
        "forge.lot.OpenLot",
        json!({"note": "n"}),
    )
    .expect("the lot opens");
    let counted = event(&opened, "forge.lot.LotCounted");
    assert!(
        counted["lot_id"].is_i64(),
        "`LotCounted.lot_id` is a declared Integer: {counted:?}"
    );
}

/// `{payload: E.mark}` over a struct filled member by member, and two sequences from two events:
/// each stored value is the one published.
#[test]
fn payload_reads_of_struct_members_and_of_two_events_store_what_was_published() {
    let mut store = MemoryStore::default();
    let mut mint = Counting(0);
    let opened = run(
        &mut store,
        &mut mint,
        "forge.lot.OpenLot",
        json!({"note": "n"}),
    )
    .expect("the lot opens");
    let first = event(&opened, "forge.lot.LotOpened").clone();
    let second = event(&opened, "forge.lot.LotCounted").clone();
    let row = store
        .load(LOT, &first["lot_id"])
        .expect("reads")
        .expect("stored")
        .row
        .fields;
    assert_eq!(row["mark"], first["mark"]);
    assert_eq!(row["first"], first["seq"]);
    assert_eq!(row["second"], second["seq"]);
    assert_ne!(row["first"], row["second"]);
    let code = first["mark"]["code"].as_str().expect("text");
    assert!(code.starts_with("cd_"), "{code}");
}

/// A generated `Timestamp` stamped from a clock past what the formatter represents must be
/// refused as a value, not panic inside the kernel.
#[test]
fn a_generated_timestamp_past_year_9999_is_refused_not_a_panic() {
    let mut store = MemoryStore::default();
    let mut mint = Counting(0);
    let lot = opened_lot(&mut store, &mut mint);
    let far = SystemTime::UNIX_EPOCH + Duration::from_secs(253_402_300_800);
    let ir = ir();
    let sent = input(json!({"lot_id": lot}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_at(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            &move || far,
            "forge.lot.StampLot",
            &sent,
        )
    }));
    match result {
        Ok(Err(ExecuteError::InvalidValue(_))) => {}
        Ok(other) => panic!("expected InvalidValue, got {other:?}"),
        Err(_) => panic!("the kernel panicked on a clock at year 10000"),
    }
}

/// A rank whose newtype declares an invariant and digits whose alphabet admits no minted text
/// are refused by name before anything runs.
#[test]
fn unmintable_newtypes_are_refused_by_name() {
    let mut store = MemoryStore::default();
    let mut mint = Counting(0);
    let lot = opened_lot(&mut store, &mut mint);
    for (command, field) in [
        ("forge.lot.RankLot", "`rank`"),
        ("forge.lot.DigitLot", "`digits`"),
    ] {
        match run(&mut store, &mut mint, command, json!({"lot_id": lot})) {
            Err(ExecuteError::Unsupported { construct }) => {
                assert!(construct.contains(field), "{construct}");
            }
            other => panic!("{command}: expected Unsupported, got {other:?}"),
        }
    }
}
