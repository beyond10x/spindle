//! A creation whose identity is neither an input nor a payload source: the runtime mints it.
//!
//! The fixture `tests/fixtures/mint` opens a ticket from a title alone, its `ticket_id` (a newtype
//! of `Uuid`) given `{generated: true}` in both events the creation emits, and a label whose
//! `String` identity nothing here mints.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Identity, MemoryStore, NoExternals, Store, Uuid, execute, execute_minting,
};

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mint");
    spindle_spec::load(&root)
        .expect("the mint fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

const MINTED: &str = "5e0c9a4b-1f2d-4c3e-9a8b-7d6c5b4a3f21";

#[test]
fn a_creation_without_an_id_input_mints_one_and_emits_it() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut asked = Vec::new();
    let mut mint = |identity: &Identity<'_>| {
        asked.push(format!(
            "{} for {}/{}: {}.{}",
            identity.capability,
            identity.command,
            identity.outcome,
            identity.entity,
            identity.field
        ));
        Uuid::parse_str(MINTED).expect("a UUID")
    };

    let opened = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "mint.ticket.OpenTicket",
        &input(json!({"title": "printer jam"})),
    )
    .expect("a creation with a minted identity runs");

    assert_eq!(
        asked,
        vec![
            "identity.mint for mint.ticket.OpenTicket/opened: mint.ticket.Ticket.ticket_id"
                .to_owned(),
            "identity.mint for mint.ticket.OpenTicket/opened: mint.ticket.TicketQueued.ticket_id"
                .to_owned(),
        ],
        "the mint is asked for the identity, and once more for `TicketQueued.ticket_id`, which \
         is not the event `instance:` names and so is a generated value of its own (ESS \
         `interpret::execute::emit`)"
    );
    assert_eq!(opened.outcome, "opened");
    assert_eq!(opened.error, None);
    let events = opened
        .events
        .iter()
        .map(|event| (event.name.as_str(), Value::Object(event.payload.clone())))
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        vec![
            (
                "mint.ticket.TicketOpened",
                json!({"ticket_id": MINTED, "title": "printer jam"})
            ),
            ("mint.ticket.TicketQueued", json!({"ticket_id": MINTED})),
        ],
        "this mint answers one UUID to every request, so both events show it"
    );

    let stored = store
        .load("mint.ticket.Ticket", &json!(MINTED))
        .expect("the store reads")
        .expect("the new instance is stored under the minted identity");
    assert_eq!(stored.row.state, "Open");
    assert_eq!(stored.row.fields.get("title"), Some(&json!("printer jam")));

    let closed = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "mint.ticket.CloseTicket",
        &input(json!({"ticket_id": MINTED})),
    )
    .expect("the minted identity addresses the instance");
    assert_eq!(closed.outcome, "closed");
    assert_eq!(
        store
            .load("mint.ticket.Ticket", &json!(MINTED))
            .expect("the store reads")
            .expect("still there")
            .row
            .state,
        "Closed"
    );
}

#[test]
fn minted_identities_are_unique_across_creations() {
    const CREATIONS: usize = 64;
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut minted = BTreeSet::new();
    for creation in 0..CREATIONS {
        let opened = execute(
            &ir,
            &mut store,
            &mut NoExternals,
            "mint.ticket.OpenTicket",
            &input(json!({"title": format!("ticket {creation}")})),
        )
        .expect("each creation runs");
        let identity = opened.events[0].payload["ticket_id"]
            .as_str()
            .expect("the identity is text")
            .to_owned();
        assert_ne!(
            opened.events[1].payload["ticket_id"], identity,
            "only the event `instance:` names carries the identity; `TicketQueued.ticket_id` is \
             minted on its own (ESS `interpret::execute::emit`)"
        );
        let parsed = Uuid::parse_str(&identity).expect("the identity is a UUID");
        assert_eq!(parsed.get_version_num(), 4, "{identity} is a UUIDv4");
        assert_eq!(
            parsed.hyphenated().to_string(),
            identity,
            "{identity} is spelled canonically"
        );
        let stored = store
            .load("mint.ticket.Ticket", &json!(identity))
            .expect("the store reads")
            .expect("each creation stores its own instance");
        assert_eq!(
            stored.row.fields.get("title"),
            Some(&json!(format!("ticket {creation}")))
        );
        minted.insert(identity);
    }
    assert_eq!(minted.len(), CREATIONS, "no identity is minted twice");
}

#[test]
fn a_creation_whose_identity_is_an_input_does_not_mint() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut mint =
        |identity: &Identity<'_>| -> Uuid { panic!("nothing is minted for {}", identity.field) };
    let filed = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "mint.ticket.FileTicket",
        &input(json!({"ticket_id": MINTED, "title": "named"})),
    )
    .expect("a creation with an input identity runs");
    assert_eq!(filed.events[0].payload["ticket_id"], json!(MINTED));
    assert!(
        store
            .load("mint.ticket.Ticket", &json!(MINTED))
            .expect("the store reads")
            .is_some()
    );
}

#[test]
fn a_minted_identity_that_is_not_a_uuid_is_unsupported_naming_the_command_and_the_field() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut mint =
        |identity: &Identity<'_>| -> Uuid { panic!("nothing is minted for {}", identity.field) };
    let error = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "mint.label.OpenLabel",
        &input(json!({"text": "fragile"})),
    )
    .expect_err("a String identity is not minted");
    match &error {
        ExecuteError::Unsupported { construct } => {
            assert!(construct.contains("mint.label.OpenLabel"), "{construct}");
            assert!(construct.contains("label_id"), "{construct}");
            assert!(construct.contains("String"), "{construct}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// The fixture `tests/fixtures/mint_text`: a badge whose identity is a `String` newtype with a
/// prefix and an alphabet, a seal whose `String` newtype declares a length invariant, and a stamp
/// whose alphabet holds digits only.
fn text_ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mint_text");
    spindle_spec::load(&root)
        .expect("the mint_text fixture compiles")
        .ir
}

const BADGE_PREFIX: &str = "bdg_";
const BADGE_ALPHABET: &str = "_0123456789abcdefg-";

fn issue(ir: &spindle_spec::EssIr, store: &mut MemoryStore, holder: &str) -> String {
    let issued = execute(
        ir,
        store,
        &mut NoExternals,
        "minttext.badge.IssueBadge",
        &input(json!({"holder": holder})),
    )
    .expect("a creation with a minted text identity runs");
    assert_eq!(issued.outcome, "issued");
    issued.events[0].payload["badge_id"]
        .as_str()
        .expect("the identity is text")
        .to_owned()
}

#[test]
fn a_string_newtype_identity_is_minted() {
    let ir = text_ir();
    let mut store = MemoryStore::default();
    let identity = issue(&ir, &mut store, "ada");

    let rest = identity
        .strip_prefix(BADGE_PREFIX)
        .unwrap_or_else(|| panic!("{identity} starts with the newtype's prefix {BADGE_PREFIX}"));
    assert!(!rest.is_empty(), "{identity} holds more than its prefix");
    assert!(
        identity.chars().all(|c| BADGE_ALPHABET.contains(c)),
        "{identity} keeps to the newtype's alphabet {BADGE_ALPHABET}"
    );

    let stored = store
        .load("minttext.badge.Badge", &json!(identity))
        .expect("the store reads")
        .expect("the new instance is stored under the minted identity");
    assert_eq!(stored.row.state, "Active");
    assert_eq!(stored.row.fields.get("holder"), Some(&json!("ada")));

    let revoked = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "minttext.badge.RevokeBadge",
        &input(json!({"badge_id": identity})),
    )
    .expect("the minted identity is a value of its type, and addresses the instance");
    assert_eq!(revoked.outcome, "revoked");
}

#[test]
fn two_creations_mint_two_identities() {
    let ir = text_ir();
    let mut store = MemoryStore::default();
    let first = issue(&ir, &mut store, "ada");
    let second = issue(&ir, &mut store, "grace");
    assert_ne!(first, second, "each creation mints its own identity");
    for (identity, holder) in [(&first, "ada"), (&second, "grace")] {
        let stored = store
            .load("minttext.badge.Badge", &json!(identity))
            .expect("the store reads")
            .expect("each creation stores its own instance");
        assert_eq!(stored.row.fields.get("holder"), Some(&json!(holder)));
    }
}

#[test]
fn a_newtype_the_runtime_cannot_satisfy_is_refused_by_name() {
    let ir = text_ir();
    for (command, field, kind) in [
        (
            "minttext.badge.OpenSeal",
            "seal_id",
            "minttext.badge.SealId",
        ),
        (
            "minttext.badge.OpenStamp",
            "stamp_id",
            "minttext.badge.StampId",
        ),
    ] {
        let mut store = MemoryStore::default();
        let mut mint = |identity: &Identity<'_>| -> Uuid {
            panic!("nothing is minted for {}", identity.field)
        };
        let error = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            command,
            &input(json!({"note": "kept"})),
        )
        .expect_err("an identity no minted text satisfies is not minted");
        match &error {
            ExecuteError::Unsupported { construct } => {
                assert!(construct.contains(command), "{construct}");
                assert!(construct.contains(field), "{construct}");
                assert!(construct.contains(kind), "{construct}");
            }
            other => panic!("{command}: expected Unsupported, got {other:?}"),
        }
    }
}
