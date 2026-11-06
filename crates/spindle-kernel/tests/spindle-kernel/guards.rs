//! Guards that read the input beside the held instance and an outside answer: an input guard
//! beside `when_subject:`, `{field, equals}` with `when:`, `external:` with `when:`, and
//! `external:` narrowed by `when_subject:`. The fixture `tests/fixtures/orders` declares one of
//! each.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

fn orders() -> EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orders");
    spindle_spec::load(&root)
        .expect("the orders fixture compiles")
        .ir
}

/// Runs `command`, answering every external question with `answer` and recording which
/// capabilities were asked.
fn run(
    ir: &EssIr,
    store: &mut MemoryStore,
    asked: &mut Vec<String>,
    answer: bool,
    command: &str,
    request: Value,
) -> Executed {
    let Value::Object(input) = request else {
        panic!("an input is an object, not {request}")
    };
    let input: Map<String, Value> = input;
    let mut decider = |question: &External<'_>| {
        asked.push(question.capability.clone());
        Ok(answer)
    };
    execute(ir, store, &mut decider, command, &input)
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn open(ir: &EssIr, store: &mut MemoryStore, order: &str, tier: &str, limit: i64) {
    let opened = run(
        ir,
        store,
        &mut Vec::new(),
        false,
        "probe.order.OpenOrder",
        json!({"order_id": order, "tier": tier, "limit": limit}),
    );
    assert_eq!(opened.outcome, "opened");
}

fn last(store: &mut MemoryStore, order: &str) -> Value {
    store
        .load("probe.order.Order", &json!(order))
        .expect("the store reads")
        .expect("the order exists")
        .row
        .fields["last"]
        .clone()
}

fn error(executed: &Executed) -> Option<&str> {
    executed.error.as_ref().map(|error| error.name.as_str())
}

#[test]
fn an_input_guard_beside_a_subject_guard_refuses_only_when_both_hold() {
    let ir = orders();
    let mut store = MemoryStore::default();
    let mut asked = Vec::new();
    open(&ir, &mut store, "o1", "Basic", 100);
    open(&ir, &mut store, "o2", "Basic", -5);

    let refused = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.Charge",
        json!({"order_id": "o1", "amount": 150}),
    );
    assert_eq!(refused.outcome, "over-limit");
    assert_eq!(error(&refused), Some("probe.order.OverLimit"));
    assert!(refused.events.is_empty());
    assert_eq!(last(&mut store, "o1"), json!(0));

    let charged = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.Charge",
        json!({"order_id": "o2", "amount": 0}),
    );
    assert_eq!(
        charged.outcome, "charged",
        "above the limit, but the input guard `amount > 0` does not hold"
    );
    assert!(asked.is_empty(), "{asked:?}");
}

#[test]
fn a_field_guard_with_an_input_guard_holds_a_large_gold_charge() {
    let ir = orders();
    let mut store = MemoryStore::default();
    let mut asked = Vec::new();
    open(&ir, &mut store, "o3", "Gold", 1000);
    open(&ir, &mut store, "o4", "Basic", 1000);

    let held = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.Charge",
        json!({"order_id": "o3", "amount": 60}),
    );
    assert_eq!(held.outcome, "held");
    assert_eq!(held.events.len(), 1);
    assert_eq!(held.events[0].name, "probe.order.ChargeHeld");
    assert_eq!(last(&mut store, "o3"), json!(60));
    assert!(asked.is_empty(), "{asked:?}");

    let basic = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.Charge",
        json!({"order_id": "o4", "amount": 60}),
    );
    assert_eq!(basic.outcome, "charged", "a basic order is not held");

    let small = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.Charge",
        json!({"order_id": "o3", "amount": 40}),
    );
    assert_eq!(small.outcome, "charged", "a small gold charge is not held");
    assert_eq!(last(&mut store, "o3"), json!(40));
}

#[test]
fn an_external_cause_with_an_input_guard_asks_only_eligible_requests() {
    let ir = orders();
    let mut store = MemoryStore::default();
    let mut asked = Vec::new();
    open(&ir, &mut store, "o1", "Basic", 100);

    let small = run(
        &ir,
        &mut store,
        &mut asked,
        true,
        "probe.order.Charge",
        json!({"order_id": "o1", "amount": 5}),
    );
    assert_eq!(small.outcome, "charged");
    assert!(
        asked.is_empty(),
        "a charge of five is not eligible: {asked:?}"
    );

    let flagged = run(
        &ir,
        &mut store,
        &mut asked,
        true,
        "probe.order.Charge",
        json!({"order_id": "o1", "amount": 20}),
    );
    assert_eq!(flagged.outcome, "flagged");
    assert_eq!(error(&flagged), Some("probe.order.ChargeFlagged"));
    assert!(flagged.events.is_empty());
    assert_eq!(asked, ["external.probe.order.Charge.flagged"]);
    assert_eq!(
        last(&mut store, "o1"),
        json!(5),
        "a refusal changes nothing"
    );

    let cleared = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.Charge",
        json!({"order_id": "o1", "amount": 20}),
    );
    assert_eq!(cleared.outcome, "charged");
    assert_eq!(last(&mut store, "o1"), json!(20));
}

#[test]
fn an_external_cause_over_the_subject_asks_only_for_the_records_it_names() {
    let ir = orders();
    let mut store = MemoryStore::default();
    let mut asked = Vec::new();
    open(&ir, &mut store, "basic", "Basic", 10);
    open(&ir, &mut store, "gold", "Gold", 10);

    let gold = run(
        &ir,
        &mut store,
        &mut asked,
        true,
        "probe.order.CloseOrder",
        json!({"order_id": "gold"}),
    );
    assert_eq!(gold.outcome, "closed");
    assert!(asked.is_empty(), "a gold order is never vetoed: {asked:?}");

    let vetoed = run(
        &ir,
        &mut store,
        &mut asked,
        true,
        "probe.order.CloseOrder",
        json!({"order_id": "basic"}),
    );
    assert_eq!(vetoed.outcome, "vetoed");
    assert_eq!(error(&vetoed), Some("probe.order.CloseVetoed"));
    assert_eq!(asked, ["external.probe.order.CloseOrder.vetoed"]);

    let closed = run(
        &ir,
        &mut store,
        &mut asked,
        false,
        "probe.order.CloseOrder",
        json!({"order_id": "basic"}),
    );
    assert_eq!(closed.outcome, "closed");
    assert_eq!(closed.events[0].name, "probe.order.OrderClosed");
}
