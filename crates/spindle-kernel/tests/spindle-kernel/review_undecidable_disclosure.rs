//! What the refusal of a row-set guard left unknown by an absent `Optional` field
//! (`story:absent-optional-in-row-set-filter`) discloses to the caller. Its text is the body of the
//! `501` an HTTP caller receives (`spindle-ingress` `answer.rs`, `Answer::unfinished`), so it
//! names nothing of a stored row the caller did not send: the story asks for the predicate, the
//! entity and the absent field, which is what the model's author needs to rule such a row out.
//!
//! The fixture is `tests/fixtures/absent-optional`: a `RetireBox` request for box `C` beside an
//! item in no box, which the caller never names.

use std::path::Path;

use serde_json::{Value, json};
use spindle_kernel::{ExecuteError, Executed, MemoryStore, NoExternals, execute};

use super::related_guards::object;

const CREATE: &str = "store.shelf.CreateBox";
const ADD: &str = "store.shelf.AddItem";
const RETIRE: &str = "store.shelf.RetireBox";

const C: &str = "0b0c0d0e-0000-4000-8000-00000000000c";
const LOOSE: &str = "e1e2e3e4-0000-4000-8000-000000000002";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/absent-optional"),
    )
    .unwrap_or_else(|error| panic!("the absent-optional fixture compiles: {error}"))
    .ir
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input))
}

/// The refusal of `RetireBox` for `C` names the filter, the entity and the absent field, and not
/// the identity of the stored item in no box, a row of another entity the request never named.
#[test]
fn review_an_undecidable_row_set_refusal_names_no_stored_row_the_caller_did_not_send() {
    let ir = ir();
    let mut store = MemoryStore::default();
    send(&ir, &mut store, CREATE, json!({"id": C, "label": "c"})).expect("the box is created");
    send(
        &ir,
        &mut store,
        ADD,
        json!({"id": LOOSE, "label": "secret"}),
    )
    .expect("the item is added in no box");
    let refused = send(&ir, &mut store, RETIRE, json!({"id": C}))
        .expect_err("an item in no box leaves the count open");
    assert_eq!(refused.kind(), "undecidable", "{refused}");
    let message = refused.to_string();
    for named in [
        "`box_id == input.id`",
        "`store.shelf.Item`",
        "`box_id` is absent",
    ] {
        assert!(message.contains(named), "{named} in: {message}");
    }
    assert!(
        !message.contains(LOOSE),
        "the refusal discloses the identity of a stored Item the request did not name: {message}"
    );
}
