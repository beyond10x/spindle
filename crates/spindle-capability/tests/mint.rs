//! A creation whose identity the runtime mints requires `identity.mint`.

use std::path::{Path, PathBuf};

use spindle_capability::{Admission, Capability, Environment, Reason, admit, derive};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn mint() -> spindle_spec::EssIr {
    spindle_spec::load(&root().join("crates/spindle-kernel/tests/fixtures/mint"))
        .expect("the mint fixture compiles")
        .ir
}

fn environment(provides: &str) -> Environment {
    Environment::parse(&format!(
        "format = \"spindle-environment/1\"\nname = \"test\"\nos = \"linux\"\narch = \"x86_64\"\n\n[provides]\n{provides}"
    ))
    .expect("the environment parses")
}

fn shipped(name: &str) -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments").join(name))
        .expect("the environment reads");
    Environment::parse(&text).expect("the environment parses")
}

#[test]
fn an_environment_without_identity_mint_refuses_such_a_specification() {
    let ir = mint();
    let requirements = derive(&ir, Some("desk")).expect("desk is a component");
    let reasons = requirements
        .get(&Capability::new(Capability::IDENTITY_MINT))
        .expect("a minted identity requires identity.mint");
    assert_eq!(
        reasons.iter().collect::<Vec<_>>(),
        vec![&Reason {
            kind: "outcome",
            name: "mint.ticket.OpenTicket/opened".to_owned(),
            because: "creates `mint.ticket.Ticket` under an identity `ticket_id` neither the \
                      input nor the outcome determines"
                .to_owned(),
        }],
        "only the creation that mints is a reason, not the one named by its input"
    );

    let without = environment("\"store.durable\" = [\"memory\"]\n");
    assert_eq!(
        admit(&requirements, &without),
        Admission::Refused {
            missing: vec![Capability::new(Capability::IDENTITY_MINT)]
        }
    );

    let with = environment("\"store.durable\" = [\"memory\"]\n\"identity.mint\" = [\"uuid-v4\"]\n");
    let Admission::Admitted { bindings } = admit(&requirements, &with) else {
        panic!("an environment that mints admits it");
    };
    assert!(
        bindings.iter().any(|binding| {
            binding.capability.as_str() == Capability::IDENTITY_MINT
                && binding.implementation == "uuid-v4"
        }),
        "{bindings:?}"
    );

    for name in ["laptop.toml", "microvm.toml"] {
        assert_eq!(
            shipped(name).implementations(Capability::IDENTITY_MINT),
            Some(&["uuid-v4".to_owned()][..]),
            "{name} binds identity.mint"
        );
    }
}

#[test]
fn a_component_that_creates_nothing_under_a_minted_identity_requires_no_mint() {
    let ir = mint();
    let whole = derive(&ir, None).expect("the system derives");
    assert!(whole.contains_key(&Capability::new(Capability::IDENTITY_MINT)));
    let rally = spindle_spec::load(&root().join("examples/rally/ess"))
        .expect("rally compiles")
        .ir;
    assert!(
        !derive(&rally, None)
            .expect("rally derives")
            .contains_key(&Capability::new(Capability::IDENTITY_MINT)),
        "every rally creation names its identity in the input"
    );
}
