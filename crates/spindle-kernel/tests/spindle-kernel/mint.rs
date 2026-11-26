//! A creation whose identity is neither an input nor a payload source: the runtime mints it.
//!
//! The fixture `tests/fixtures/mint` opens a ticket from a title alone, its `ticket_id` (a newtype
//! of `Uuid`) given `{generated: true}` in both events the creation emits, and a label whose bare
//! `String` identity the runtime mints as a UUID's text with no prefix.

use std::collections::BTreeSet;
use std::path::Path;

use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Identity, MemoryStore, Mint, NoExternals, Seeded, Store, Uuid, execute,
    execute_minting,
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
fn a_bare_string_identity_is_minted_as_a_text_without_a_prefix() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut asked = Vec::new();
    let mut mint = |identity: &Identity<'_>| {
        asked.push(format!("{}.{}", identity.entity, identity.field));
        Uuid::parse_str(MINTED).expect("a UUID")
    };
    let opened = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "mint.label.OpenLabel",
        &input(json!({"text": "fragile"})),
    )
    .expect("a bare String identity is minted");
    assert_eq!(asked, vec!["mint.label.Label.label_id".to_owned()]);
    assert_eq!(opened.outcome, "opened");
    assert_eq!(
        opened.events[0].payload["label_id"],
        json!(MINTED),
        "a bare `String` identity is the minted UUID's text, with no prefix"
    );
    let stored = store
        .load("mint.label.Label", &json!(MINTED))
        .expect("the store reads")
        .expect("the new instance is stored under the minted identity");
    assert_eq!(stored.row.fields.get("text"), Some(&json!("fragile")));
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

/// The fixture `tests/fixtures/mint_types`: a note whose identity is a bare `String`, a counter
/// whose identity is an `Integer`, and a seat whose identity is a newtype of `Integer` without
/// invariants, each opened from one input with `{generated: true}` on its identity; and a chip
/// (`Bytes`) and a mark (`Timestamp`) opened the same way, whose minted identities would repeat.
fn types_ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mint_types");
    spindle_spec::load(&root)
        .expect("the mint_types fixture compiles")
        .ir
}

/// Whether `identity` is a value of the minted identity's type: an integer for an `Integer` or
/// a newtype of one, and for a bare `String` a minted text with no prefix, a UUID's text.
fn of_type(identity: &Value, integer: bool) -> bool {
    if integer {
        return identity.is_i64();
    }
    identity.as_str().is_some_and(|text| {
        Uuid::parse_str(text).is_ok_and(|uuid| uuid.hyphenated().to_string() == text)
    })
}

#[test]
fn an_identity_of_each_minted_type_is_minted() {
    let ir = types_ir();
    for (command, entity, identity_field, input_field, integer) in [
        (
            "minttypes.note.OpenNote",
            "minttypes.note.Note",
            "note_id",
            "body",
            false,
        ),
        (
            "minttypes.counter.OpenCounter",
            "minttypes.counter.Counter",
            "counter_id",
            "name",
            true,
        ),
        (
            "minttypes.seat.OpenSeat",
            "minttypes.seat.Seat",
            "seat_number",
            "holder",
            true,
        ),
    ] {
        let spec = ir
            .commands()
            .values()
            .find(|spec| spec.name.to_string() == command)
            .unwrap_or_else(|| panic!("the fixture declares {command}"));
        spindle_kernel::runs(&ir, spec)
            .unwrap_or_else(|error| panic!("{command} is admitted at start: {error}"));

        let mut store = MemoryStore::default();
        let mut minted = Vec::new();
        for sent in ["first", "second"] {
            let opened = execute(
                &ir,
                &mut store,
                &mut NoExternals,
                command,
                &input(json!({input_field: sent})),
            )
            .unwrap_or_else(|error| panic!("{command} mints its identity: {error}"));
            assert_eq!(opened.outcome, "opened", "{command}");
            assert_eq!(opened.error, None, "{command}");
            let identity = opened.events[0].payload[identity_field].clone();
            assert!(
                of_type(&identity, integer),
                "{command}: {identity} is a value of the identity's type"
            );
            let stored = store
                .load(entity, &identity)
                .expect("the store reads")
                .unwrap_or_else(|| panic!("{command}: the instance is stored under {identity}"));
            assert_eq!(stored.row.fields.get(input_field), Some(&json!(sent)));
            minted.push(identity);
        }
        assert_ne!(
            minted[0], minted[1],
            "{command}: two creates mint two identities"
        );
    }
}

#[test]
fn an_identity_a_mint_would_repeat_is_refused_at_start() {
    let ir = types_ir();
    for (command, field, kind) in [
        ("minttypes.repeat.OpenChip", "chip_id", "Bytes"),
        ("minttypes.repeat.OpenMark", "marked_at", "Timestamp"),
    ] {
        let spec = ir
            .commands()
            .values()
            .find(|spec| spec.name.to_string() == command)
            .unwrap_or_else(|| panic!("the fixture declares {command}"));
        match spindle_kernel::runs(&ir, spec) {
            Err(ExecuteError::Unsupported { construct }) => {
                assert!(construct.contains(command), "{construct}");
                assert!(construct.contains(field), "{construct}");
                assert!(construct.contains(kind), "{construct}");
            }
            other => panic!("{command}: a {kind} identity is refused at start, not {other:?}"),
        }
    }
}

/// The fixture `tests/fixtures/fallback`: a parcel registered under the caller's id whose label
/// falls back to the literal `unlabelled`, its priority to the literal `3`, its route to the
/// input `depot`, its sequence (`Integer`) to `{generated: true}`, and its memo
/// (`Optional<String>`) to `{generated: true}`, each in `sets:` and in the event.
fn fallback_ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fallback");
    spindle_spec::load(&root)
        .expect("the fallback fixture compiles")
        .ir
}

const REGISTER: &str = "fallback.parcel.RegisterParcel";
const SENT: &str = "1a1a1a1a-0000-4000-8000-000000000001";
const BARE: &str = "2b2b2b2b-0000-4000-8000-000000000002";

/// `value` read as ESS's [`Node`]: the interpreter writes an integral `Integer` as `7.0` and the
/// kernel as `7`, one number either way.
fn node(field: &str, value: &Value) -> Node {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|_| panic!("`{field}` holds a node, not {value}"))
}

/// Asserts that the kernel's `spindle` and the interpreter's `ess` hold the same fields with the
/// same values, but for `sequence` where `minted`: present on both sides, the kernel's an
/// integer and the interpreter's an integral number, values each mints for itself.
fn agree(what: &str, spindle: &Map<String, Value>, ess: &Map<String, Value>, minted: bool) {
    let fields = spindle.keys().chain(ess.keys()).collect::<BTreeSet<_>>();
    for field in fields {
        let (ours, theirs) = (spindle.get(field), ess.get(field));
        if minted && field == "sequence" {
            assert!(
                ours.is_some_and(Value::is_i64),
                "{what}: the kernel mints an Integer `{field}`: {ours:?}"
            );
            assert!(
                theirs
                    .and_then(Value::as_f64)
                    .is_some_and(|number| number.fract() == 0.0),
                "{what}: the interpreter mints an Integer `{field}`: {theirs:?}"
            );
        } else {
            assert_eq!(
                ours.map(|value| node(field, value)),
                theirs.map(|value| node(field, value)),
                "{what}: `{field}` is the value the interpreter answers"
            );
        }
    }
}

#[test]
fn a_value_source_fallback_answers_as_the_interpreter() {
    let ir = fallback_ir();
    let views = [(
        "fallback.parcel.ParcelCard",
        "fallback.parcel.Parcel",
        "parcel_id",
    )];
    let steps = [
        (
            REGISTER,
            json!({
                "parcel_id": SENT,
                "label": "fragile",
                "priority": 7,
                "route": "north",
                "depot": "east",
                "sequence": 41,
                "memo": "by hand",
            }),
        ),
        (REGISTER, json!({"parcel_id": BARE, "depot": "east"})),
    ];
    let (ess_answers, ess_rows) = super::related_guards::interpreted(&ir, &views, &steps);
    let (answers, rows) = super::related_guards::spindled(&ir, &views, &steps);

    // The first request sends every primary input; the second sends none, so its label and
    // priority are the literals, its route the other input, its sequence minted and its memo,
    // an `Optional` minted without `present: true`, absent (`interpret::values::mint`).
    assert_eq!(answers.len(), ess_answers.len());
    for (step, (answer, ess)) in answers.iter().zip(&ess_answers).enumerate() {
        assert_eq!(
            (&answer.0, &answer.1),
            (&ess.0, &ess.1),
            "step {step}: the outcome"
        );
        assert_eq!(answer.2.len(), 1, "step {step}: {answer:?}");
        assert_eq!(ess.2.len(), 1, "step {step}: {ess:?}");
        assert_eq!(answer.2[0].0, ess.2[0].0, "step {step}: the event");
        agree(
            &format!("the event of step {step}"),
            &answer.2[0].1,
            &ess.2[0].1,
            step == 1,
        );
    }
    let mut fallen = answers[1].2[0].1.clone();
    assert!(
        fallen
            .remove("sequence")
            .is_some_and(|value| value.is_i64())
    );
    assert_eq!(
        Value::Object(fallen),
        json!({"parcel_id": BARE, "label": "unlabelled", "priority": 3, "route": "east"}),
        "the request without the primary inputs falls back to the literals and the other input, \
         and leaves the Optional memo absent"
    );

    assert_eq!((rows.len(), ess_rows.len()), (1, 1));
    let (rows, ess_rows) = (&rows[0], &ess_rows[0]);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(ess_rows.len(), 2, "{ess_rows:?}");
    for (step, (row, ess_row)) in rows.iter().zip(ess_rows).enumerate() {
        agree(&format!("the row of step {step}"), row, ess_row, step == 1);
    }
}

/// The fixture `tests/fixtures/mint_redraw`: a tally under an `Integer` identity, opened by
/// `OpenTally` with `{generated: true}` on it, and by `OpenTallyAs` with the identity the request
/// sends, else `{generated: true}`.
fn redraw_ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mint_redraw");
    spindle_spec::load(&root)
        .expect("the mint_redraw fixture compiles")
        .ir
}

const TALLY: &str = "mintredraw.tally.Tally";

/// The nil UUID and `1 << 53` share their low 53 bits, so both mint the `Integer` 0: the second
/// creation through the `{generated: true}` fallback draws again and opens under 7, as the
/// creation that mints its identity directly does.
#[test]
fn a_fallback_minted_integer_identity_an_instance_holds_is_drawn_again() {
    let ir = redraw_ir();
    let mut store = MemoryStore::default();
    let mut uuids = vec![Uuid::nil(), Uuid::from_u128(1 << 53), Uuid::from_u128(7)].into_iter();
    let mut mint = |_: &Identity<'_>| uuids.next().expect("a UUID left");
    let mut minted = Vec::new();
    for name in ["first", "second"] {
        let opened = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            "mintredraw.tally.OpenTallyAs",
            &input(json!({"name": name})),
        )
        .unwrap_or_else(|error| panic!("{name}: the tally opens: {error}"));
        let identity = opened.events[0].payload["tally_id"].clone();
        let stored = store
            .load(TALLY, &identity)
            .expect("the store reads")
            .unwrap_or_else(|| panic!("{name}: stored under {identity}"));
        assert_eq!(stored.row.fields.get("name"), Some(&json!(name)));
        minted.push(identity);
    }
    assert_eq!(minted, [json!(0), json!(7)]);
}

/// A mint that answers one UUID on every draw, with `held` tallies stored, 0 among them: the next
/// creation draws one more identity than the tallies held, as ESS's interpreter draws
/// `0..=occupied` (`interpret::execute::creation_identity`), each minting the held 0, and is
/// refused naming the draws; the stored tallies are kept.
#[test]
fn an_integer_identity_held_on_every_draw_is_refused_after_one_draw_more_than_the_held() {
    let ir = redraw_ir();
    for held in [1_u64, 3] {
        let mut store = MemoryStore::default();
        let mut unasked = |identity: &Identity<'_>| -> Uuid {
            panic!("a supplied identity draws nothing: {identity:?}")
        };
        for tally in 0..held {
            execute_minting(
                &ir,
                &mut store,
                &mut NoExternals,
                &mut unasked,
                "mintredraw.tally.OpenTallyAs",
                &input(json!({"tally_id": tally, "name": "held"})),
            )
            .unwrap_or_else(|error| panic!("tally {tally} opens: {error}"));
        }
        let mut asked = 0_usize;
        let mut mint = |_: &Identity<'_>| {
            asked += 1;
            Uuid::nil()
        };
        let error = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            "mintredraw.tally.OpenTally",
            &input(json!({"name": "next"})),
        )
        .expect_err("every draw mints the held 0");
        let draws = usize::try_from(held).expect("a small count") + 1;
        match &error {
            ExecuteError::Undeclared { reason, .. } => {
                assert!(
                    reason.contains(&format!("each of {draws} `Integer` identities")),
                    "{held} held: {reason}"
                );
                assert!(reason.contains(TALLY), "{reason}");
            }
            other => {
                panic!("a held identity on every draw is refused as Undeclared, not {other:?}")
            }
        }
        assert_eq!(asked, draws, "{held} held: one draw more than the held");
        let kept = store.scan(TALLY).expect("the store scans").rows;
        assert_eq!(
            kept.len(),
            usize::try_from(held).expect("a small count"),
            "{kept:?}"
        );
    }
}

/// An `Integer` identity the request sends is not drawn again: a held one refuses the creation,
/// and the mint is not asked.
#[test]
fn a_held_integer_identity_the_request_sends_is_refused_without_a_draw() {
    let ir = redraw_ir();
    let mut store = MemoryStore::default();
    let mut asked = 0_usize;
    let mut mint = |_: &Identity<'_>| {
        asked += 1;
        Uuid::from_u128(5)
    };
    for (name, first) in [("first", true), ("again", false)] {
        let opened = execute_minting(
            &ir,
            &mut store,
            &mut NoExternals,
            &mut mint,
            "mintredraw.tally.OpenTallyAs",
            &input(json!({"tally_id": 5, "name": name})),
        );
        match (opened, first) {
            (Ok(_), true) => {}
            (Err(ExecuteError::Undeclared { reason, .. }), false) => {
                assert!(reason.contains("already has the identity 5"), "{reason}");
            }
            (other, _) => panic!("{name}: {other:?}"),
        }
    }
    assert_eq!(
        asked, 0,
        "the mint is not asked for an identity the request sends"
    );
}

/// A seeded mint told a row holds the `Integer` its first UUID mints, spelled `n` or `n.0`, or
/// the text that UUID is or ends in, skips that UUID and answers the one after it.
#[test]
fn a_seeded_mint_skips_a_uuid_whose_integer_or_text_an_identity_holds() {
    let at = Identity {
        capability: "identity.mint",
        command: "mintredraw.tally.OpenTally",
        outcome: "opened",
        entity: TALLY,
        field: "tally_id",
    };
    let mut plain = Seeded::new(7);
    let first = plain.uuid(&at);
    let second = plain.uuid(&at);
    let low = u64::try_from(first.as_u128() & ((1 << 53) - 1)).expect("53 bits");
    #[allow(
        clippy::cast_precision_loss,
        reason = "53 bits, which a double holds exactly"
    )]
    let spelled = low as f64;
    for held in [
        json!(low),
        json!(spelled),
        json!(format!("prefix_{first}")),
        json!(first.to_string()),
    ] {
        let mut avoiding = Seeded::new(7).avoiding_identities([&held]);
        assert_eq!(avoiding.uuid(&at), second, "held {held}");
    }
    let mut unheld = Seeded::new(7).avoiding_identities([&json!(low + 1), &json!(true)]);
    assert_eq!(
        unheld.uuid(&at),
        first,
        "an identity no first UUID mints skips nothing"
    );
}
