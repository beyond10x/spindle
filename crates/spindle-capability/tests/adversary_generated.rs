//! Adversary cases for `story:generated-field-values`: derivation must name what the kernel mints
//! and nothing the kernel refuses (`derive.rs` `generated_mints`: "a type of any other kind is no
//! mint the runtime has, and the kernel refuses it by name").

use std::path::{Path, PathBuf};

use spindle_capability::{Capability, derive};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lots() -> spindle_spec::EssIr {
    spindle_spec::load(&root().join("crates/spindle-kernel/tests/fixtures/adversary_generated"))
        .expect("the adversary fixture compiles")
        .ir
}

/// `RankLot` (a newtype of `Integer` with an invariant) and `DigitLot` (a `String` newtype whose
/// alphabet admits no minted text) are refused by the kernel as `unsupported`
/// (`tests/adversary_generated.rs` in spindle-kernel); derivation still names `identity.mint` for
/// them.
#[test]
fn derivation_names_no_mint_for_a_generated_field_the_kernel_refuses() {
    let requirements = derive(&lots(), Some("yard")).expect("yard is a component");
    let minted = requirements
        .get(&Capability::new(Capability::IDENTITY_MINT))
        .expect("OpenLot mints");
    let refused: Vec<_> = minted
        .iter()
        .filter(|reason| {
            reason.name == "forge.lot.RankLot/ranked" || reason.name == "forge.lot.DigitLot/digited"
        })
        .cloned()
        .collect();
    assert!(
        refused.is_empty(),
        "identity.mint is derived for fields the kernel refuses to mint: {refused:?}"
    );
}

/// The kernel mints every generated struct member; derivation names each, with the place.
#[test]
fn derivation_names_generated_struct_members_and_the_timestamp_member_needs_clock() {
    let requirements = derive(&lots(), Some("yard")).expect("yard is a component");
    let clock = requirements
        .get(&Capability::new(Capability::CLOCK))
        .expect("a generated Timestamp member requires clock");
    assert!(
        clock.iter().any(
            |reason| reason.name == "forge.lot.OpenLot/opened" && reason.because.contains("at")
        ),
        "{clock:?}"
    );
    assert!(
        clock
            .iter()
            .any(|reason| reason.name == "forge.lot.StampLot/stamped"),
        "{clock:?}"
    );
}
