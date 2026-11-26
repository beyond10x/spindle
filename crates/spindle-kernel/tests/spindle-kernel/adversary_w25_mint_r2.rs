//! Adversary cases, round 2, for `story:identity-mint-types-and-fallback`: a minted `Integer`
//! identity an instance already holds is drawn again (`creation_identity`, at most one draw more
//! than the instances held), also through `{input: f, else: {generated: true}}` by rewinding the
//! generator, and a seeded mint skips a UUID whose low 53 bits a world row holds
//! (`Seeded::avoiding_identities`).
//!
//! The fixture `tests/fixtures/adversary_w25_mint_r2` declares a tally under an `Integer`
//! identity, opened with a minted token, a one-time ticket and `sets:` that read the token and the
//! identity back (`OpenTally`, `OpenTallyAs`), opened with nothing else minted (`OpenPlain`,
//! `OpenPlainAs`); an other under another `Integer` identity; and a badge under an
//! `Integer` newtype without invariants. Expected answers come from ESS 0.99.1's interpreter
//! (`ess_conformance::interpret`) where it reads the same construct.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Executed, Identity, MemoryStore, Mint, NoExternals, Seeded, SqliteStore, Store,
    Uuid, execute_minting,
};

const TALLY: &str = "advw25r2.tally.Tally";
const OTHER: &str = "advw25r2.tally.Other";
const BADGE: &str = "advw25r2.tally.Badge";
const OPEN_TALLY: &str = "advw25r2.tally.OpenTally";
const OPEN_TALLY_AS: &str = "advw25r2.tally.OpenTallyAs";
const OPEN_PLAIN: &str = "advw25r2.tally.OpenPlain";
const OPEN_PLAIN_AS: &str = "advw25r2.tally.OpenPlainAs";
const OPEN_OTHER: &str = "advw25r2.tally.OpenOther";
const OPEN_BADGE: &str = "advw25r2.tally.OpenBadge";
const OPEN_BADGE_AS: &str = "advw25r2.tally.OpenBadgeAs";
const ALL_TALLIES: &str = "advw25r2.tally.AllTallies";

const TOKEN: u128 = 0x1111_1111_1111_4111_8111_1111_1111_1111;
const TICKET: u128 = 0x2222_2222_2222_4222_8222_2222_2222_2222;

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_w25_mint_r2");
    spindle_spec::load(&root)
        .unwrap_or_else(|error| panic!("the adversary_w25_mint_r2 fixture compiles: {error:?}"))
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

/// A mint that answers its UUIDs in turn and records the field each draw was for.
struct Scripted {
    uuids: VecDeque<Uuid>,
    asked: Vec<String>,
}

impl Scripted {
    fn new(uuids: impl IntoIterator<Item = u128>) -> Self {
        Self {
            uuids: uuids.into_iter().map(Uuid::from_u128).collect(),
            asked: Vec::new(),
        }
    }
}

impl Mint for Scripted {
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid {
        self.asked.push(identity.field.to_owned());
        self.uuids
            .pop_front()
            .unwrap_or_else(|| panic!("no UUID left for `{}`", identity.field))
    }
}

fn run(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    mint: &mut dyn Mint,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute_minting(ir, store, &mut NoExternals, mint, command, &object(input))
}

fn ok(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    mint: &mut dyn Mint,
    command: &str,
    input: Value,
) -> Executed {
    run(ir, store, mint, command, input.clone())
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn integer(uuid: u128) -> u64 {
    u64::try_from(uuid & ((1 << 53) - 1)).expect("53 bits")
}

/// Each stored identity of `entity`, by its number.
fn numbers(store: &mut dyn Store, entity: &str) -> Vec<f64> {
    store
        .scan(entity)
        .expect("the store scans")
        .rows
        .iter()
        .map(|(identity, _)| {
            identity
                .as_f64()
                .unwrap_or_else(|| panic!("a numeric identity, not {identity}"))
        })
        .collect()
}

// ---- the interpreter -------------------------------------------------------------------------

struct Ess {
    target: Interpreted,
    context: ScenarioContext,
}

impl Ess {
    fn new(ir: &spindle_spec::EssIr) -> Self {
        let target = Interpreted::for_model(ir.clone());
        let context = ScenarioContext::new(
            "advw25r2.tally/authored/redraw"
                .parse()
                .expect("a scenario id"),
            CorrelationId::new("advw25r2").expect("a correlation id"),
        );
        target.begin_scenario(&context).expect("the scenario opens");
        Self { target, context }
    }

    /// The payloads of the events `command` emits, or the interpreter's refusal.
    fn run(&self, command: &str, input: &Value) -> Result<Vec<BTreeMap<String, Node>>, String> {
        let input = object(input.clone())
            .into_iter()
            .map(|(field, value)| {
                (
                    field,
                    serde_json::from_value(value).expect("an input value is a node"),
                )
            })
            .collect();
        self.target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: self.context.correlation.clone(),
            })
            .map(|executed| {
                executed
                    .direct_events
                    .into_iter()
                    .map(|event| event.payload)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    fn rows(&self, view: &str) -> Vec<BTreeMap<String, Node>> {
        self.target
            .query_view(SemanticViewRequest {
                view: ViewRef::new(view.parse().expect("a view name")),
                params: BTreeMap::new(),
                consistency: ess_primitives::consistency::QueryConsistency::Current,
                correlation: self.context.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .expect("the interpreter reads the view")
            .rows
    }
}

/// A node's number, as written: the interpreter spells an integral number `7.0` or `7`.
fn node_number(node: &Node) -> f64 {
    let text = serde_json::to_value(node)
        .expect("a node is JSON")
        .to_string();
    text.trim_matches('"')
        .parse()
        .unwrap_or_else(|_| panic!("a number, not {text}"))
}

// ---- the redraw keeps every other value --------------------------------------------------------

/// Asserts a tally opened after one redraw: the token, the one-time ticket and the stored copies
/// keep their own draws, and the identity, its event field and `copy` read the redrawn 7.
fn assert_redrawn(store: &mut dyn Store, mint: &Scripted, opened: &Executed) {
    assert_eq!(
        mint.asked,
        ["token", "tally_id", "tally_id", "ticket"],
        "the token `sets:` reads first (`publish_reads`), then the identity twice, then the \
         response (drawn before the identity only where the identity reads it; ESS's interpreter \
         answers a response from its own authority, not from the counter)"
    );
    let payload = &opened.events[0].payload;
    assert_eq!(payload["tally_id"], json!(7), "{payload:?}");
    assert_eq!(
        payload["token"],
        json!(Uuid::from_u128(TOKEN).to_string()),
        "{payload:?}"
    );
    let response = opened.response.as_ref().expect("the outcome returns");
    assert_eq!(
        response["ticket"],
        json!(Uuid::from_u128(TICKET).to_string()),
        "{response:?}"
    );
    let stored = store
        .load(TALLY, &json!(7))
        .expect("the store reads")
        .expect("the tally is stored under the redrawn 7");
    assert_eq!(stored.row.fields.get("copy"), Some(&json!(7)), "{stored:?}");
    assert_eq!(
        stored.row.fields.get("stamp"),
        Some(&json!(Uuid::from_u128(TOKEN).to_string())),
        "{stored:?}"
    );
    assert_eq!(numbers(store, TALLY), [0.0, 7.0]);
}

/// `{generated: true}` on the identity: one held draw (the nil UUID mints 0) and one redraw.
#[test]
fn adv_r2_a_minted_redraw_keeps_every_other_draw_and_sets_read_the_redrawn_identity() {
    let ir = ir();
    let mut store = MemoryStore::default();
    ok(
        &ir,
        &mut store,
        &mut Scripted::new([0]),
        OPEN_PLAIN,
        json!({"name": "held"}),
    );
    let mut mint = Scripted::new([TOKEN, 0, 7, TICKET]);
    let opened = ok(
        &ir,
        &mut store,
        &mut mint,
        OPEN_TALLY,
        json!({"name": "next"}),
    );
    assert_redrawn(&mut store, &mint, &opened);
}

/// `{input: tally_id, else: {generated: true}}` without the input: the generator is rewound to
/// redraw the identity, and must keep the token `sets:` read before it.
#[test]
fn adv_r2_a_fallback_redraw_keeps_every_other_draw_and_sets_read_the_redrawn_identity() {
    let ir = ir();
    let mut store = MemoryStore::default();
    ok(
        &ir,
        &mut store,
        &mut Scripted::new([0]),
        OPEN_PLAIN,
        json!({"name": "held"}),
    );
    let mut mint = Scripted::new([TOKEN, 0, 7, TICKET]);
    let opened = ok(
        &ir,
        &mut store,
        &mut mint,
        OPEN_TALLY_AS,
        json!({"name": "next"}),
    );
    assert_redrawn(&mut store, &mint, &opened);
}

// ---- what counts as held -----------------------------------------------------------------------

/// The interpreter on the minted case: a tally the request opened as `X.0`, where `X` is the
/// integer the interpreter mints first, makes the next minted creation draw again.
#[test]
fn adv_r2_the_interpreter_draws_again_on_a_float_spelled_held_integer() {
    let ir = ir();
    let first = Ess::new(&ir)
        .run(OPEN_PLAIN, &json!({"name": "probe"}))
        .expect("the interpreter mints");
    let minted = node_number(&first[0]["tally_id"]);

    let ess = Ess::new(&ir);
    ess.run(OPEN_PLAIN_AS, &json!({"tally_id": minted, "name": "float"}))
        .expect("the interpreter opens the float-spelled tally");
    let next = ess
        .run(OPEN_PLAIN, &json!({"name": "minted"}))
        .expect("the interpreter mints again");
    assert_ne!(node_number(&next[0]["tally_id"]), minted);
    assert_eq!(ess.rows(ALL_TALLIES).len(), 2);
}

/// An `Integer` newtype identity without invariants is drawn again like an `Integer`, through
/// `{generated: true}` and through the fallback.
#[test]
fn adv_r2_an_integer_newtype_identity_held_is_drawn_again() {
    let ir = ir();
    for command in [OPEN_BADGE, OPEN_BADGE_AS] {
        let mut store = MemoryStore::default();
        ok(
            &ir,
            &mut store,
            &mut Scripted::new([0]),
            OPEN_BADGE,
            json!({"name": "held"}),
        );
        let mut mint = Scripted::new([0, 7]);
        let opened = ok(&ir, &mut store, &mut mint, command, json!({"name": "next"}));
        assert_eq!(opened.events[0].payload["badge_no"], json!(7), "{command}");
        assert_eq!(mint.asked.len(), 2, "{command}: {:?}", mint.asked);
        assert_eq!(numbers(&mut store, BADGE), [0.0, 7.0], "{command}");
    }
}

/// An other holding 0 does not hold a tally's 0: the tally opens under 0 on its first draw.
#[test]
fn adv_r2_an_integer_held_by_another_entity_is_not_drawn_again() {
    let ir = ir();
    let mut store = MemoryStore::default();
    ok(
        &ir,
        &mut store,
        &mut Scripted::new([0]),
        OPEN_OTHER,
        json!({"name": "other"}),
    );
    let mut mint = Scripted::new([0]);
    let opened = ok(
        &ir,
        &mut store,
        &mut mint,
        OPEN_PLAIN,
        json!({"name": "tally"}),
    );
    assert_eq!(opened.events[0].payload["tally_id"], json!(0));
    assert_eq!(mint.asked, ["tally_id"]);
    assert_eq!(numbers(&mut store, OTHER), [0.0]);
    assert_eq!(numbers(&mut store, TALLY), [0.0]);
}

// ---- the bound -------------------------------------------------------------------------------

/// With eight tallies held under the first eight integers the mint draws, the interpreter draws a
/// ninth (`creation_identity`: one candidate beyond the held population) and opens the tally.
/// The kernel bounds its draws the same way: the held population plus one.
#[test]
fn adv_r2_eight_held_draws_with_eight_instances_held_open_on_the_ninth_as_the_interpreter() {
    let ir = ir();
    let probe = Ess::new(&ir);
    let drawn = (0..9)
        .map(|_| {
            let events = probe
                .run(OPEN_PLAIN, &json!({"name": "probe"}))
                .expect("the interpreter mints");
            node_number(&events[0]["tally_id"])
        })
        .collect::<Vec<_>>();
    let ess = Ess::new(&ir);
    for held in &drawn[..8] {
        ess.run(OPEN_PLAIN_AS, &json!({"tally_id": held, "name": "held"}))
            .expect("the interpreter opens a supplied tally");
    }
    let ninth = ess
        .run(OPEN_PLAIN, &json!({"name": "ninth"}))
        .expect("the interpreter draws past eight held identities");
    assert_eq!(node_number(&ninth[0]["tally_id"]), drawn[8]);

    let mut store = MemoryStore::default();
    let mut none = Scripted::new([]);
    for held in 1..=8_u64 {
        ok(
            &ir,
            &mut store,
            &mut none,
            OPEN_PLAIN_AS,
            json!({"tally_id": held, "name": "held"}),
        );
    }
    let mut mint = Scripted::new(1..=9);
    let opened = run(
        &ir,
        &mut store,
        &mut mint,
        OPEN_PLAIN,
        json!({"name": "ninth"}),
    );
    match opened {
        Ok(opened) => assert_eq!(opened.events[0].payload["tally_id"], json!(9)),
        Err(error) => panic!(
            "eight instances held, the interpreter opens the tally on its ninth draw; the kernel \
             refused after {} draws: {error}",
            mint.asked.len()
        ),
    }
}

// ---- durable store and seeded mint -------------------------------------------------------------

/// A tally held on a durable store before a restart is held after it: the creation draws again.
#[test]
fn adv_r2_a_held_integer_on_a_reopened_sqlite_store_is_drawn_again() {
    let ir = ir();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("tally.sqlite");
    let mut store = SqliteStore::open(&path).expect("the store opens");
    ok(
        &ir,
        &mut store,
        &mut Scripted::new([0]),
        OPEN_PLAIN,
        json!({"name": "held"}),
    );
    drop(store);
    let mut reopened = SqliteStore::open(&path).expect("the store reopens");
    let mut mint = Scripted::new([0, 7]);
    let opened = ok(
        &ir,
        &mut reopened,
        &mut mint,
        OPEN_PLAIN,
        json!({"name": "next"}),
    );
    assert_eq!(opened.events[0].payload["tally_id"], json!(7));
    assert_eq!(numbers(&mut reopened, TALLY), [0.0, 7.0]);
}

/// A seeded mint told a row holds its first integer answers its second, then its third: the skip
/// shifts later draws by one and never rewinds, and two such mints answer alike.
#[test]
fn adv_r2_a_seeded_skip_shifts_every_later_draw_by_one_and_stays_deterministic() {
    let ir = ir();
    let at = Identity {
        capability: "identity.mint",
        command: OPEN_PLAIN,
        outcome: "opened",
        entity: TALLY,
        field: "tally_id",
    };
    let mut plain = Seeded::new(7);
    let drawn = (0..3)
        .map(|_| integer(plain.uuid(&at).as_u128()))
        .collect::<Vec<_>>();
    let mut answers = Vec::new();
    for _ in 0..2 {
        let mut store = MemoryStore::default();
        let mut seeded = Seeded::new(7).avoiding_identities([&json!(drawn[0])]);
        let opened = (0..2)
            .map(|_| {
                ok(
                    &ir,
                    &mut store,
                    &mut seeded,
                    OPEN_PLAIN,
                    json!({"name": "seeded"}),
                )
                .events[0]
                    .payload["tally_id"]
                    .clone()
            })
            .collect::<Vec<_>>();
        answers.push(opened);
    }
    assert_eq!(answers[0], [json!(drawn[1]), json!(drawn[2])]);
    assert_eq!(answers[0], answers[1]);
}
