//! An error field the refusing outcome gives no source is the runtime's to fill: from the row the
//! refusal is answered for where that row holds it, as ESS's served application reads it, and
//! minted from its type otherwise, as ESS's interpreter leaves it to the implementation. Only the
//! minted ones require `identity.mint`.

use std::path::{Path, PathBuf};

use spindle_capability::{Capability, Reason, derive};

fn desk() -> spindle_spec::EssIr {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    spindle_spec::load(&root.join("crates/spindle-ingress/tests/fixtures/desk"))
        .expect("the desk fixture compiles")
        .ir
}

#[test]
fn an_error_field_nothing_fills_requires_identity_mint_and_a_held_one_does_not() {
    let requirements = derive(&desk(), Some("desk")).expect("desk is a component");
    let minted = requirements
        .get(&Capability::new(Capability::IDENTITY_MINT))
        .expect("the weekend refusal's note is minted");
    assert_eq!(
        minted.iter().cloned().collect::<Vec<_>>(),
        vec![Reason {
            kind: "outcome",
            name: "study.desk.BookDesk/closed".to_owned(),
            because: "mints `study.desk.ClosedOnWeekends.note`, which the outcome gives no source"
                .to_owned(),
        }],
        "`AlreadyTaken.holder` is read from the taken desk and mints nothing"
    );
}

#[test]
fn a_default_refusal_read_from_the_row_and_an_unknown_instance_refusal_mint_nothing() {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ir = spindle_spec::load(&root.join("crates/spindle-kernel/tests/fixtures/held-refusal"))
        .expect("the held-refusal fixture compiles")
        .ir;
    let requirements = derive(&ir, Some("loan")).expect("loan is a component");
    assert_eq!(
        requirements.get(&Capability::new(Capability::IDENTITY_MINT)),
        None,
        "`Unavailable.keeper` is read from the addressed book and `NoSuchBook.book_id` is left \
         out for a book that does not exist"
    );
}
