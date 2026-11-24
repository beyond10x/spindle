//! Values read from the one row a predicate selects (ess/22): `{related: {entity, where, field}}`
//! in `sets:` or `payload:` reads `field` of the one row of `entity` whose stored fields satisfy
//! `where`, as the store held it before the outcome, as ESS's interpreter reads it
//! (`interpret::execute::row_set::Reader::one`, `determined::selected`). No selected row, or
//! more than one, gives no value, and nothing is written.
//!
//! The fixture `tests/fixtures/guild` knows members by their address. `FoundTeam` founds a team
//! where an active member holds the address and copies that member's id and `Optional` nickname
//! from the one member row with the address; `DraftTeam` copies the id unguarded. `Rejoin`
//! creates a member and names the one the address had before, and `Refresh` copies the address of
//! the member the team's stored captain id names (`member_id == subject.captain_id`).

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Removal, Row, Scan, SqliteStore,
    Store, StoreError, Stored, execute,
};

use super::related_guards::{Answer, interpreted, object, spindled};

const MEMBER: &str = "guild.roster.Member";
const TEAM: &str = "guild.roster.Team";
const ENROL: &str = "guild.roster.Enrol";
const LEAVE: &str = "guild.roster.Leave";
const REJOIN: &str = "guild.roster.Rejoin";
const FOUND: &str = "guild.roster.FoundTeam";
const DRAFT: &str = "guild.roster.DraftTeam";
const REFRESH: &str = "guild.roster.Refresh";
const DEPART: &str = "guild.roster.Depart";

const ADA: &str = "0a0a0a0a-0000-4000-8000-000000000001";
const BEA: &str = "0a0a0a0a-0000-4000-8000-000000000002";
const CY: &str = "0a0a0a0a-0000-4000-8000-000000000003";
const ADA_AGAIN: &str = "0a0a0a0a-0000-4000-8000-000000000004";
const ADA_THIRD: &str = "0a0a0a0a-0000-4000-8000-000000000005";
const RED: &str = "7e7e7e7e-0000-4000-8000-000000000001";
const BLUE: &str = "7e7e7e7e-0000-4000-8000-000000000002";
const GREEN: &str = "7e7e7e7e-0000-4000-8000-000000000003";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/guild"))
        .expect("the guild fixture compiles")
        .ir
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    command: &str,
    input: &Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input.clone()))
}

fn run(ir: &spindle_spec::EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    send(ir, store, command, &input)
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn enrol(ir: &spindle_spec::EssIr, store: &mut dyn Store, member: &str, email: &str) {
    let enrolled = run(
        ir,
        store,
        ENROL,
        json!({"member_id": member, "email": email}),
    );
    assert_eq!(enrolled.outcome, "enrolled");
}

fn row(store: &mut dyn Store, entity: &str, identity: &str) -> Option<Row> {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
        .map(|stored| stored.row)
}

/// Refuses unless `result` is the kernel's no-value answer.
fn no_value(result: Result<Executed, ExecuteError>, what: &str) {
    match result {
        Err(ExecuteError::NoValue { .. }) => {}
        other => panic!("{what}: no value, not {other:?}"),
    }
}

/// Whether ESS's interpreter answers `steps` in order, the last one included, without refusing
/// to determine one: `false` where the last step determines no value.
fn interpreter_determines(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> bool {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "guild.roster/authored/where"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("where").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut last = true;
    for (command, input) in steps {
        let input = object(input.clone())
            .into_iter()
            .map(|(field, value)| {
                let node: Node = serde_json::from_value(value).expect("a node");
                (field, node)
            })
            .collect();
        last = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            })
            .is_ok_and(|executed| executed.outcome.is_some());
    }
    last
}

#[test]
fn a_value_reads_the_one_row_a_predicate_selects() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let enrolled = run(
        &ir,
        &mut store,
        ENROL,
        json!({"member_id": ADA, "email": "ada@example.org", "nickname": "Ace"}),
    );
    assert_eq!(enrolled.outcome, "enrolled");
    enrol(&ir, &mut store, BEA, "bea@example.org");

    let founded = run(
        &ir,
        &mut store,
        FOUND,
        json!({"team_id": RED, "captain_email": "ada@example.org"}),
    );
    assert_eq!(founded.outcome, "captain-found");
    assert_eq!(founded.events.len(), 1);
    assert_eq!(founded.events[0].name, "guild.roster.TeamFounded");
    assert_eq!(founded.events[0].payload["captain_id"], json!(ADA));
    assert_eq!(founded.events[0].payload["captain_nickname"], json!("Ace"));
    let team = row(&mut store, TEAM, RED).expect("the team is founded");
    assert_eq!(team.fields["captain_id"], json!(ADA));
    assert_eq!(team.fields["captain_nickname"], json!("Ace"));

    let founded = run(
        &ir,
        &mut store,
        FOUND,
        json!({"team_id": BLUE, "captain_email": "bea@example.org"}),
    );
    assert_eq!(founded.outcome, "captain-found");
    assert_eq!(
        founded.events[0].payload["captain_id"],
        json!(BEA),
        "each request reads the row its own address selects"
    );
    assert_eq!(
        founded.events[0].payload.get("captain_nickname"),
        None,
        "an `Optional` field the selected row holds nothing in stays absent"
    );
    let team = row(&mut store, TEAM, BLUE).expect("the team is founded");
    assert_eq!(team.fields.get("captain_nickname"), None);

    let refused = run(
        &ir,
        &mut store,
        FOUND,
        json!({"team_id": GREEN, "captain_email": "nobody@example.org"}),
    );
    assert_eq!(refused.outcome, "no-captain");
    assert_eq!(row(&mut store, TEAM, GREEN), None);
}

#[test]
fn no_selected_row_or_several_give_no_value_and_write_nothing() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let draft = json!({"team_id": RED, "captain_email": "ada@example.org"});
    no_value(
        send(&ir, &mut store, DRAFT, &draft),
        "an unguarded read where no row is selected",
    );
    assert_eq!(row(&mut store, TEAM, RED), None);
    assert!(store.events().is_empty(), "nothing is written");

    enrol(&ir, &mut store, ADA, "ada@example.org");
    enrol(&ir, &mut store, BEA, "ada@example.org");
    let events = store.events().len();
    no_value(
        send(&ir, &mut store, DRAFT, &draft),
        "an unguarded read where two rows are selected",
    );
    assert_eq!(row(&mut store, TEAM, RED), None);
    assert_eq!(store.events().len(), events, "nothing is written");

    let left = run(&ir, &mut store, LEAVE, json!({"member_id": ADA}));
    assert_eq!(left.outcome, "left");
    let events = store.events().len();
    no_value(
        send(
            &ir,
            &mut store,
            FOUND,
            &json!({"team_id": BLUE, "captain_email": "ada@example.org"}),
        ),
        "the guard holds of the one active member, and the value's own selector selects two",
    );
    assert_eq!(row(&mut store, TEAM, BLUE), None);
    assert_eq!(store.events().len(), events, "nothing is written");

    for steps in [
        vec![(DRAFT, draft.clone())],
        vec![
            (ENROL, json!({"member_id": ADA, "email": "ada@example.org"})),
            (ENROL, json!({"member_id": BEA, "email": "ada@example.org"})),
            (DRAFT, draft.clone()),
        ],
    ] {
        assert!(
            !interpreter_determines(&ir, &steps),
            "ESS's interpreter determines no value either: {steps:?}"
        );
    }
}

#[test]
fn the_selection_reads_the_store_before_the_outcome() {
    let ir = ir();
    let mut store = MemoryStore::default();
    enrol(&ir, &mut store, ADA, "ada@example.org");
    run(&ir, &mut store, LEAVE, json!({"member_id": ADA}));

    let rejoined = run(
        &ir,
        &mut store,
        REJOIN,
        json!({"member_id": ADA_AGAIN, "email": "ada@example.org"}),
    );
    assert_eq!(rejoined.outcome, "rejoined");
    assert_eq!(
        rejoined.events[0].payload["former_id"],
        json!(ADA),
        "the row the outcome creates holds the address too, and is no candidate of its own read"
    );
    assert_eq!(
        row(&mut store, MEMBER, ADA_AGAIN)
            .expect("the member rejoined")
            .fields["former_id"],
        json!(ADA)
    );

    no_value(
        send(
            &ir,
            &mut store,
            REJOIN,
            &json!({"member_id": ADA_THIRD, "email": "ada@example.org"}),
        ),
        "two rows held the address before the outcome",
    );
    assert_eq!(row(&mut store, MEMBER, ADA_THIRD), None);
}

#[test]
fn a_selector_reads_the_addressed_subject() {
    let ir = ir();
    let mut store = MemoryStore::default();
    enrol(&ir, &mut store, ADA, "ada@example.org");
    enrol(&ir, &mut store, BEA, "bea@example.org");
    run(
        &ir,
        &mut store,
        FOUND,
        json!({"team_id": RED, "captain_email": "bea@example.org"}),
    );
    let refreshed = run(&ir, &mut store, REFRESH, json!({"team_id": RED}));
    assert_eq!(refreshed.outcome, "refreshed");
    assert_eq!(
        refreshed.events[0].payload["captain_email"],
        json!("bea@example.org"),
        "the member whose id the team stores is selected"
    );
    assert_eq!(
        row(&mut store, TEAM, RED).expect("the team").fields["captain_email"],
        json!("bea@example.org")
    );
}

/// A memory store that counts the scans of each entity: what a selection reads.
#[derive(Default)]
struct Scans {
    inner: MemoryStore,
    scanned: Vec<String>,
}

impl Store for Scans {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.scanned.push(entity.to_owned());
        self.inner.scan(entity)
    }
}

/// `Depart` selects its successor in `departed` alone. A request its `wrong-state` refusal
/// answers reads no selection: no member is scanned, so a refusal costs no read of the rows and
/// conditions nothing on them. The branch that reads it scans the members once.
#[test]
fn a_selection_is_read_only_for_the_branch_that_reads_it() {
    let ir = ir();
    let mut store = Scans::default();
    enrol(&ir, &mut store, ADA, "ada@example.org");
    enrol(&ir, &mut store, BEA, "bea@example.org");
    run(&ir, &mut store, LEAVE, json!({"member_id": ADA}));
    store.scanned.clear();

    let refused = run(
        &ir,
        &mut store,
        DEPART,
        json!({"member_id": ADA, "successor_email": "bea@example.org"}),
    );
    assert_eq!(refused.outcome, "wrong-state");
    assert_eq!(
        store.scanned,
        Vec::<String>::new(),
        "the refusal reads no selection"
    );

    let departed = run(
        &ir,
        &mut store,
        DEPART,
        json!({"member_id": BEA, "successor_email": "ada@example.org"}),
    );
    assert_eq!(departed.outcome, "departed");
    assert_eq!(store.scanned, vec![MEMBER.to_owned()]);
    assert_eq!(
        row(&mut store.inner, MEMBER, BEA)
            .expect("the member")
            .fields["former_id"],
        json!(ADA)
    );
}

const VIEWS: [(&str, &str, &str); 2] = [
    ("guild.roster.MemberCard", MEMBER, "member_id"),
    ("guild.roster.TeamCard", TEAM, "team_id"),
];

/// Every step answers: each selected read selects one row, or its guard refuses.
fn steps() -> Vec<(&'static str, Value)> {
    vec![
        (
            ENROL,
            json!({"member_id": ADA, "email": "ada@example.org", "nickname": "Ace"}),
        ),
        (ENROL, json!({"member_id": BEA, "email": "bea@example.org"})),
        (ENROL, json!({"member_id": CY, "email": "cy@example.org"})),
        (
            FOUND,
            json!({"team_id": RED, "captain_email": "ada@example.org"}),
        ),
        (
            FOUND,
            json!({"team_id": BLUE, "captain_email": "bea@example.org"}),
        ),
        (
            FOUND,
            json!({"team_id": GREEN, "captain_email": "nobody@example.org"}),
        ),
        (LEAVE, json!({"member_id": CY})),
        (
            FOUND,
            json!({"team_id": GREEN, "captain_email": "cy@example.org"}),
        ),
        (
            DRAFT,
            json!({"team_id": GREEN, "captain_email": "cy@example.org"}),
        ),
        (
            REJOIN,
            json!({"member_id": ADA_AGAIN, "email": "cy@example.org"}),
        ),
        (REFRESH, json!({"team_id": RED})),
        (REFRESH, json!({"team_id": GREEN})),
    ]
}

#[test]
fn where_selected_values_agree_with_the_interpreter() {
    let ir = ir();
    let steps = steps();
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "enrolled",
            "enrolled",
            "enrolled",
            "captain-found",
            "captain-found",
            "no-captain",
            "left",
            "no-captain",
            "drafted",
            "rejoined",
            "refreshed",
            "refreshed"
        ]
    );
    let (answers, rows) = spindled(&ir, &VIEWS, &steps);
    assert_eq!(
        answers, ess_answers,
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(rows, ess_rows, "the stored members and teams agree");
}

/// The answers of [`steps`] on `store`.
fn answered(ir: &spindle_spec::EssIr, store: &mut dyn Store) -> Vec<Answer> {
    steps()
        .into_iter()
        .map(|(command, input)| {
            let executed = run(ir, store, command, input);
            (
                executed.outcome,
                executed.error.map(|error| error.name),
                executed
                    .events
                    .into_iter()
                    .map(|event| (event.name, event.payload))
                    .collect(),
            )
        })
        .collect()
}

fn fields(store: &mut dyn Store, entity: &str) -> Vec<(Value, Map<String, Value>)> {
    store
        .scan(entity)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| (identity, stored.row.fields.into_iter().collect()))
        .collect()
}

#[test]
fn where_selected_values_on_sqlite_agree_with_memory() {
    let ir = ir();
    let mut memory = MemoryStore::default();
    let expected = answered(&ir, &mut memory);
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("guild.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        assert_eq!(answered(&ir, &mut store), expected);
    }
    let mut store = SqliteStore::open(&path).expect("SQLite reopens");
    for entity in [MEMBER, TEAM] {
        assert_eq!(fields(&mut store, entity), fields(&mut memory, entity));
    }
    no_value(
        send(
            &ir,
            &mut store,
            REJOIN,
            &json!({"member_id": ADA_THIRD, "email": "cy@example.org"}),
        ),
        "two rows hold the address after a restart",
    );
}

#[cfg(feature = "postgres")]
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn where_selected_values_on_postgres_agree_with_memory() {
    let ir = ir();
    let mut memory = MemoryStore::default();
    let expected = answered(&ir, &mut memory);
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let component = format!(
        "guild-{}",
        uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .bytes()
            .take(12)
            .map(|byte| char::from(b'a' + byte % 26))
            .collect::<String>()
    );
    let mut store =
        spindle_kernel::PostgresStore::open(&spindle_kernel::Database::new(url), &component, 1)
            .expect("the store opens");
    assert_eq!(answered(&ir, &mut store), expected);
    for entity in [MEMBER, TEAM] {
        assert_eq!(fields(&mut store, entity), fields(&mut memory, entity));
    }
}
