//! A field an outcome leaves to the runtime, `{generated: true}`, requires what mints it:
//! `identity.mint` for a `Uuid`, a `String` or an `Integer`, `clock` for a `Timestamp`.

use std::path::{Path, PathBuf};

use spindle_capability::{Admission, Capability, Environment, Reason, admit, derive};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn generated() -> spindle_spec::EssIr {
    spindle_spec::load(&root().join("crates/spindle-kernel/tests/fixtures/generated"))
        .expect("the generated fixture compiles")
        .ir
}

fn environment(provides: &str) -> Environment {
    Environment::parse(&format!(
        "format = \"spindle-environment/1\"\nname = \"test\"\nos = \"linux\"\narch = \"x86_64\"\n\n[provides]\n{provides}"
    ))
    .expect("the environment parses")
}

fn reason(outcome: &str, because: &str) -> Reason {
    Reason {
        kind: "outcome",
        name: outcome.to_owned(),
        because: because.to_owned(),
    }
}

#[test]
fn a_generated_field_requires_identity_mint_and_a_generated_timestamp_requires_clock() {
    let ir = generated();
    let requirements = derive(&ir, Some("kiln")).expect("kiln is a component");

    let minted = requirements
        .get(&Capability::new(Capability::IDENTITY_MINT))
        .expect("a generated Uuid, String or Integer requires identity.mint");
    let started = "forge.batch.StartBatch/started";
    let mut expected = vec![
        reason(started, "mints `forge.batch.Batch.serial`"),
        reason(started, "mints `forge.batch.Batch.token`"),
        reason(
            started,
            "mints `forge.batch.Batch.label` where the input sends none",
        ),
        reason(started, "mints `forge.batch.Batch.note`"),
        reason(started, "mints `forge.batch.BatchStarted.sequence`"),
        reason(
            "forge.batch.RelabelBatch/blank",
            "mints `forge.batch.BlankLabel.incident`",
        ),
    ];
    expected.sort();
    assert_eq!(
        minted.iter().cloned().collect::<Vec<_>>(),
        expected,
        "every generated field the runtime mints from a UUID is a reason; `{{payload: …}}` in \
         `sets:` mints nothing of its own, and a `Decimal` is no mint the runtime has"
    );

    let clock = requirements
        .get(&Capability::new(Capability::CLOCK))
        .expect("a generated Timestamp requires clock");
    assert_eq!(
        clock.iter().cloned().collect::<Vec<_>>(),
        vec![reason(
            started,
            "stamps `forge.batch.Batch.fired_at` with the current time"
        )]
    );

    let without_clock =
        environment("\"store.durable\" = [\"memory\"]\n\"identity.mint\" = [\"uuid-v4\"]\n");
    assert_eq!(
        admit(&requirements, &without_clock),
        Admission::Refused {
            missing: vec![Capability::new(Capability::CLOCK)]
        }
    );
    let without_mint = environment("\"store.durable\" = [\"memory\"]\nclock = [\"system\"]\n");
    assert_eq!(
        admit(&requirements, &without_mint),
        Admission::Refused {
            missing: vec![Capability::new(Capability::IDENTITY_MINT)]
        }
    );
    let both = environment(
        "\"store.durable\" = [\"memory\"]\n\"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\n",
    );
    assert!(
        matches!(admit(&requirements, &both), Admission::Admitted { .. }),
        "an environment that mints and keeps time admits it"
    );
}
