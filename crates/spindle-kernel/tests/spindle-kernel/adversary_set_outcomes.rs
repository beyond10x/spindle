//! Adversary cases for `story:kernel-set-outcomes`: a set outcome (`instances:`) whose store write
//! fails on a later row, `{generated: true}` in a set outcome's `sets:` and in an event published
//! once per changed row, integer identities, and `when_state:` on a per-row event.
//!
//! The loans cases run over `tests/fixtures/loans`; the revocation cases, a second writer giving
//! a grant between a set outcome's scan and its commit, over `tests/fixtures/adversary-revoke`;
//! the rest over a desk of tickets written into a scratch directory by [`desk`].

use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, Executed, MemoryStore, NoExternals, Removal, Store, StoreError, Stored, execute,
};

use super::related_guards::{interpreted, spindled};

const LOAN: &str = "lending.loans.Loan";
const LEND: &str = "lending.loans.Lend";
const MARK: &str = "lending.loans.MarkOverdue";
const LOAN_MARKED: &str = "lending.loans.LoanMarked";

const JANUARY: &str = "2026-01-15T00:00:00Z";
const FEBRUARY: &str = "2026-02-15T00:00:00Z";
const MARCH: &str = "2026-03-01T00:00:00Z";

const TICKET: &str = "probe.desk.Ticket";
const FILE: &str = "probe.desk.File";
const START: &str = "probe.desk.Start";
const CLOSE_TEAM: &str = "probe.desk.CloseTeam";
const STAMP_TEAM: &str = "probe.desk.StampTeam";
const STAMPED: &str = "probe.desk.Stamped";

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn loans() -> spindle_spec::EssIr {
    spindle_spec::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loans"),
    )
    .expect("the loans fixture compiles")
    .ir
}

const DESK_SYSTEM: &str = "format: ess/34
system: probe
version: v1
summary: A desk of tickets filed by team, started, closed and stamped together.
domains:
  - probe.desk
components:
  - component: desk
    summary: Files, starts, closes and stamps tickets.
    owns:
      domains:
        - probe.desk
    accepts:
      commands:
        - probe.desk.File
        - probe.desk.Start
        - probe.desk.CloseTeam
        - probe.desk.StampTeam
    publishes:
      events:
        - probe.desk.Filed
        - probe.desk.Started
        - probe.desk.Closed
        - probe.desk.Left
        - probe.desk.Stamped
    reached_by: in_process
";

const DESK_DOMAIN: &str = "domain: probe.desk
summary: A ticket is open, busy or closed; a team's tickets are closed or stamped together.
entities:
  - name: probe.desk.Ticket
    identity:
      name: ticket_id
      type: Integer
    fields:
      - name: team
        type: String
      - name: token
        type: Uuid
    lifecycle:
      initial: Open
      states: [Open, Busy, Closed]
      terminal: [Closed]
      transitions:
        - {name: start, from: [Open], to: Busy}
        - {name: close, from: [Open, Busy], to: Closed}
errors:
  - name: probe.desk.NotOpen
    summary: The ticket is not open.
    fields: []
commands:
  - name: probe.desk.File
    input:
      - {name: ticket_id, type: Integer}
      - {name: team, type: String}
    outcomes:
      - name: filed
        creates: probe.desk.Ticket
        instance: ticket_id
        sets:
          team: input.team
          token: {generated: true}
        emits: [probe.desk.Filed]
        payload:
          probe.desk.Filed:
            ticket_id: input.ticket_id
        summary: The ticket is filed for the team.
  - name: probe.desk.Start
    input:
      - {name: ticket_id, type: Integer}
    outcomes:
      - name: started
        moves: probe.desk.Ticket.start
        instance: ticket_id
        emits: [probe.desk.Started]
        payload:
          probe.desk.Started:
            ticket_id: input.ticket_id
        summary: Work on the ticket starts.
      - name: not-open
        wrong_state: true
        error: probe.desk.NotOpen
        summary: The ticket is not open.
  - name: probe.desk.CloseTeam
    input:
      - {name: team, type: String}
    outcomes:
      - name: closed
        moves: probe.desk.Ticket.close
        instances: {where: team == input.team}
        emits:
          - probe.desk.Closed
          - {event: probe.desk.Left, each: changed, when_state: Busy}
        payload:
          probe.desk.Closed:
            team: input.team
            count: {count: changed}
          probe.desk.Left:
            ticket_id: {changed: ticket_id}
            was: {changed: state}
        summary: Every open or busy ticket of the team is closed; a busy one is published as left.
  - name: probe.desk.StampTeam
    input:
      - {name: team, type: String}
    outcomes:
      - name: stamped
        updates: probe.desk.Ticket
        instances: {where: team == input.team}
        sets:
          token: {generated: true}
        emits:
          - {event: probe.desk.Stamped, each: changed}
        payload:
          probe.desk.Stamped:
            ticket_id: {changed: ticket_id}
            receipt: {generated: true}
        summary: Every ticket of the team takes a fresh token, each with a receipt of its own.
events:
  - name: probe.desk.Filed
    fields:
      - {name: ticket_id, type: Integer}
  - name: probe.desk.Started
    fields:
      - {name: ticket_id, type: Integer}
  - name: probe.desk.Closed
    fields:
      - {name: team, type: String}
      - {name: count, type: Integer}
  - name: probe.desk.Left
    fields:
      - {name: ticket_id, type: Integer}
      - {name: was, type: probe.desk.Ticket.State}
  - name: probe.desk.Stamped
    fields:
      - {name: ticket_id, type: Integer}
      - {name: receipt, type: Uuid}
views:
  - name: probe.desk.TicketCard
    source: probe.desk.Ticket
    consistency: read_your_writes
    fields:
      - {name: ticket_id, type: Integer}
      - {name: team, type: String}
      - {name: state, type: probe.desk.Ticket.State}
";

/// The desk model, compiled from a scratch directory.
fn desk() -> spindle_spec::EssIr {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    std::fs::write(dir.path().join("system.yaml"), DESK_SYSTEM).expect("the system is written");
    std::fs::write(dir.path().join("domains/desk.yaml"), DESK_DOMAIN)
        .expect("the domain is written");
    spindle_spec::load(dir.path())
        .expect("the desk model compiles")
        .ir
}

fn run(ir: &spindle_spec::EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    execute(ir, store, &mut NoExternals, command, &object(input.clone()))
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn file(ir: &spindle_spec::EssIr, store: &mut dyn Store, ticket: i64, team: &str) {
    let filed = run(ir, store, FILE, json!({"ticket_id": ticket, "team": team}));
    assert_eq!(filed.outcome, "filed");
}

/// A store in which another writer returns `target` between the kernel's read and its write of
/// that row, once: the commit naming the version read then conflicts.
struct Interfering {
    inner: MemoryStore,
    target: Value,
    armed: bool,
}

impl Store for Interfering {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        if self.armed && change.identity == self.target && change.expected.is_some() {
            self.armed = false;
            let held = self
                .inner
                .load(&change.entity, &change.identity)?
                .expect("the interfered row is held");
            let mut row = held.row;
            row.state = "Returned".to_owned();
            self.inner.commit(&Change {
                entity: change.entity.clone(),
                identity: change.identity.clone(),
                expected: Some(held.version),
                row,
                events: Vec::new(),
                scanned: Vec::new(),
            })?;
        }
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.inner.scan(entity)
    }
}

/// Two due loans, the second returned by another writer while `MarkOverdue` runs.
fn interfered(ir: &spindle_spec::EssIr) -> Interfering {
    let mut store = Interfering {
        inner: MemoryStore::default(),
        target: json!("loan-2"),
        armed: false,
    };
    for (loan, due) in [("loan-1", JANUARY), ("loan-2", FEBRUARY)] {
        let lent = run(
            ir,
            &mut store,
            LEND,
            json!({"loan_id": loan, "borrower": "ada", "due": due}),
        );
        assert_eq!(lent.outcome, "lent");
    }
    store.armed = true;
    store
}

fn loan_state(store: &mut dyn Store, loan: &str) -> String {
    store
        .load(LOAN, &json!(loan))
        .expect("the store reads")
        .unwrap_or_else(|| panic!("{loan} is held"))
        .row
        .state
}

/// `execute` promises that a command that does not run leaves the store unchanged. A set outcome
/// whose write of its second row conflicts answers an error, and must not leave its first row
/// moved.
#[test]
fn a_set_outcome_refused_on_a_later_row_leaves_the_store_unchanged() {
    let ir = loans();
    let mut store = interfered(&ir);

    let marked = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        MARK,
        &object(json!({"as_of": MARCH})),
    );

    assert!(
        marked.is_err(),
        "the second loan's write conflicts: {marked:?}"
    );
    assert_eq!(
        loan_state(&mut store, "loan-1"),
        "Out",
        "a command that does not run leaves the store unchanged, the first loan included"
    );
}

/// Every row a set outcome moves has its `LoanMarked` published, by the run that moved it or by a
/// retry. A conflict on the second row leaves the first moved with no event recorded and none
/// answered, and the retry no longer selects it: its event is lost.
#[test]
fn every_row_a_set_outcome_moves_has_its_per_row_event_published() {
    let ir = loans();
    let mut store = interfered(&ir);

    let first = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        MARK,
        &object(json!({"as_of": MARCH})),
    );
    let retried = run(&ir, &mut store, MARK, json!({"as_of": MARCH}));
    let moved = loan_state(&mut store, "loan-1") == "Overdue";

    let answered = first
        .iter()
        .chain([&retried])
        .flat_map(|executed| executed.events.iter());
    let published: Vec<&Value> = store
        .inner
        .events()
        .iter()
        .chain(answered)
        .filter(|event| event.name == LOAN_MARKED)
        .filter_map(|event| event.payload.get("loan_id"))
        .collect();
    if moved {
        assert!(
            published.contains(&&json!("loan-1")),
            "loan-1 was moved to Overdue, and no LoanMarked for it was ever recorded or answered \
             (published: {published:?})"
        );
    }
}

/// ESS's interpreter mints every generated value it reads, one per row a set outcome writes:
/// each stamped ticket takes a token of its own.
#[test]
fn each_row_a_set_outcome_writes_takes_its_own_generated_value() {
    let ir = desk();
    let mut store = MemoryStore::default();
    file(&ir, &mut store, 1, "a");
    file(&ir, &mut store, 2, "a");

    let stamped = run(&ir, &mut store, STAMP_TEAM, json!({"team": "a"}));
    assert_eq!(stamped.outcome, "stamped");

    let token = |store: &mut MemoryStore, ticket: i64| {
        store
            .load(TICKET, &json!(ticket))
            .expect("the store reads")
            .expect("the ticket is held")
            .row
            .fields
            .get("token")
            .cloned()
            .expect("a token is held")
    };
    let one = token(&mut store, 1);
    let two = token(&mut store, 2);
    assert_ne!(
        one, two,
        "two rows stamped by one set outcome hold one minted token between them"
    );
}

/// An event published once per changed row mints its `{generated: true}` field once per
/// occurrence, as ESS's `emit` does: each receipt is its own.
#[test]
fn each_per_row_event_mints_its_own_generated_value() {
    let ir = desk();
    let mut store = MemoryStore::default();
    file(&ir, &mut store, 1, "a");
    file(&ir, &mut store, 2, "a");

    let stamped = run(&ir, &mut store, STAMP_TEAM, json!({"team": "a"}));

    let receipts: Vec<&Value> = stamped
        .events
        .iter()
        .filter(|event| event.name == STAMPED)
        .map(|event| event.payload.get("receipt").expect("a receipt"))
        .collect();
    assert_eq!(receipts.len(), 2, "one Stamped per stamped ticket");
    assert_ne!(
        receipts[0], receipts[1],
        "two Stamped occurrences carry one minted receipt between them"
    );
}

/// Integer identities are taken in ascending numeric order (ESS orders the `Node`s), not in the
/// order of their JSON text, which puts 10 before 2.
#[test]
fn integer_identities_are_taken_in_numeric_order() {
    let ir = desk();
    let mut store = MemoryStore::default();
    for ticket in [10, 9, 2] {
        file(&ir, &mut store, ticket, "a");
    }

    let stamped = run(&ir, &mut store, STAMP_TEAM, json!({"team": "a"}));

    let order: Vec<&Value> = stamped
        .events
        .iter()
        .filter(|event| event.name == STAMPED)
        .map(|event| event.payload.get("ticket_id").expect("a ticket"))
        .collect();
    assert_eq!(order, [&json!(2), &json!(9), &json!(10)]);
}

/// A per-row event with `when_state:` is published only for the changed rows that rested in one
/// of its states before the outcome, as ESS's interpreter publishes it.
#[test]
fn per_row_events_with_when_state_agree_with_the_interpreter() {
    let ir = desk();
    let views = [("probe.desk.TicketCard", TICKET, "ticket_id")];
    let steps: Vec<(&str, Value)> = vec![
        (FILE, json!({"ticket_id": 3, "team": "a"})),
        (FILE, json!({"ticket_id": 1, "team": "a"})),
        (FILE, json!({"ticket_id": 2, "team": "b"})),
        (FILE, json!({"ticket_id": 4, "team": "a"})),
        (START, json!({"ticket_id": 3})),
        (START, json!({"ticket_id": 2})),
        (CLOSE_TEAM, json!({"team": "a"})),
        (CLOSE_TEAM, json!({"team": "a"})),
        (CLOSE_TEAM, json!({"team": "b"})),
    ];
    let (ess_answers, ess_rows) = interpreted(&ir, &views, &steps);
    let (answers, rows) = spindled(&ir, &views, &steps);

    let numbers = |answers: Vec<super::related_guards::Answer>| -> Vec<_> {
        answers
            .into_iter()
            .map(|(outcome, error, events)| {
                let events: Vec<_> = events
                    .into_iter()
                    .map(|(name, payload)| {
                        let payload: Map<String, Value> = payload
                            .into_iter()
                            .map(|(field, value)| {
                                let value = value
                                    .as_f64()
                                    .filter(|_| value.is_number())
                                    .map_or(value, |number| json!(number));
                                (field, value)
                            })
                            .collect();
                        (name, payload)
                    })
                    .collect();
                (outcome, error, events)
            })
            .collect()
    };
    assert_eq!(numbers(answers), numbers(ess_answers));
    let numbered = |rows: super::related_guards::Rows| -> Vec<Vec<Map<String, Value>>> {
        rows.into_iter()
            .map(|view| {
                view.into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|(field, value)| {
                                let value = value
                                    .as_f64()
                                    .filter(|_| value.is_number())
                                    .map_or(value, |number| json!(number));
                                (field, value)
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect()
    };
    let mut ours = numbered(rows);
    let mut theirs = numbered(ess_rows);
    for view in ours.iter_mut().chain(theirs.iter_mut()) {
        view.sort_by_key(|row| row.get("ticket_id").map(ToString::to_string));
        for row in view.iter_mut() {
            row.remove("token");
        }
    }
    assert_eq!(
        ours, theirs,
        "the stored tickets agree with ESS's view rows"
    );
}

const GRANT: &str = "probe.access.Grant";
const GIVE_GRANT: &str = "probe.access.GiveGrant";
const REVOKE_ALL: &str = "probe.access.RevokeAll";

/// `tests/fixtures/adversary-revoke`: grants given one at a time, and `RevokeAll`, a set outcome
/// removing every grant of a person.
fn revoke() -> spindle_spec::EssIr {
    spindle_spec::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-revoke"),
    )
    .expect("the adversary-revoke fixture compiles")
    .ir
}

/// A store that, right after the kernel's first scan of the grants, lets `other` (a second
/// handle on the same database) give the grant `racing` names, then goes on.
struct Granting<'s> {
    inner: &'s mut dyn Store,
    other: &'s mut dyn Store,
    ir: &'s spindle_spec::EssIr,
    racing: Option<Value>,
}

impl Store for Granting<'_> {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn commit_all(&mut self, writes: &[spindle_kernel::Write]) -> Result<(), StoreError> {
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        let scan = self.inner.scan(entity)?;
        if entity == GRANT
            && let Some(input) = self.racing.take()
        {
            run(self.ir, self.other, GIVE_GRANT, input);
        }
        Ok(scan)
    }
}

/// The grants of `person` `store` holds, by identity.
fn grants_of(store: &mut dyn Store, person: &str) -> Vec<Value> {
    store
        .scan(GRANT)
        .expect("the store scans")
        .into_iter()
        .filter(|(_, stored)| stored.row.fields.get("person") == Some(&json!(person)))
        .map(|(identity, _)| identity)
        .collect()
}

/// Revokes every grant of `ann` while a second handle gives her the grant `g9` between the
/// kernel's scan of the grants and its commit. Run serially either way, `ann` keeps no grant:
/// revoked first, `g9` is given after; given first, the revocation removes it. So either the
/// revocation is refused as a conflict and every grant of hers stands, or none does.
fn a_grant_given_during_a_revocation_does_not_survive_it(
    name: &str,
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    other: &mut dyn Store,
) {
    for (grant, person) in [("g1", "ann"), ("g2", "ann"), ("g3", "bob")] {
        run(
            ir,
            store,
            GIVE_GRANT,
            json!({"grant_id": grant, "person": person}),
        );
    }
    let mut granting = Granting {
        inner: store,
        other,
        ir,
        racing: Some(json!({"grant_id": "g9", "person": "ann"})),
    };
    let revoked = execute(
        ir,
        &mut granting,
        &mut NoExternals,
        REVOKE_ALL,
        &object(json!({"person": "ann"})),
    );
    assert!(granting.racing.is_none(), "{name}: the second writer ran");
    let held = grants_of(store, "ann");
    match revoked {
        Err(spindle_kernel::ExecuteError::Store(StoreError::Conflict { .. })) => assert_eq!(
            held,
            [json!("g1"), json!("g2"), json!("g9")],
            "{name}: a refused revocation leaves every grant of ann"
        ),
        Ok(executed) => {
            assert_eq!(executed.outcome, "revoked", "{name}");
            assert_eq!(
                held,
                Vec::<Value>::new(),
                "{name}: the revocation landed after g9 was given and left ann holding it: a \
                 set outcome's write group carries no condition on the scan that selected its \
                 rows"
            );
        }
        Err(other) => panic!("{name}: the revocation answers {other:?}"),
    }
    assert_eq!(
        grants_of(store, "bob"),
        [json!("g3")],
        "{name}: bob's grant is never touched"
    );
}

#[test]
fn a_grant_given_during_a_revocation_does_not_survive_it_on_sqlite() {
    let ir = revoke();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("access.sqlite");
    let mut store = spindle_kernel::SqliteStore::open(&path).expect("SQLite opens");
    let mut other =
        spindle_kernel::SqliteStore::open(&path).expect("a second handle on the file opens");
    a_grant_given_during_a_revocation_does_not_survive_it("sqlite", &ir, &mut store, &mut other);
}

#[cfg(feature = "postgres")]
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_grant_given_during_a_revocation_does_not_survive_it_on_postgres() {
    let ir = revoke();
    let (mut store, mut other) = super::adversary_secondary_effects::postgres_pair();
    a_grant_given_during_a_revocation_does_not_survive_it("postgres", &ir, &mut store, &mut other);
}
