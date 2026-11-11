//! Adversary cases for secondary effects (`affects:`, `story:kernel-secondary-effects`).
//!
//! The fixture `tests/fixtures/adversary-affects` holds accounts and their sessions.
//! `OpenSession` opens a session only for an account that exists (`when_related`).
//! `CloseAccount` removes the account and every session of it: a group whose every write is a
//! removal. `BumpAccount` raises the account's level from an expression, sets each of its
//! sessions' hits to the amount, ends every session that had no hits (a row two entries select)
//! and sets every other account below it to the amount (an entry over the subject's own entity,
//! its filter reading `subject.`). ESS's compiler refuses `{expr: …}` in an `affects:` entry's
//! `sets:` (`unsupported_construct`), so the entries write input values and literals.
//!
//! The race cases put a second writer, a second handle on the same database, between the
//! kernel's scan of the sessions and its commit: the second writer opens a session of the account
//! being closed. Run serially either way, no session of a removed account survives: closed first,
//! the session is refused for want of an account; opened first, the close removes it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, NoExternals, Removal, Row, Scan, SqliteStore, Store,
    StoreError, Stored, Write, execute,
};

use super::related_guards::{Answer, Rows, interpreted, spindled_with};

const ACCOUNT: &str = "tenancy.accounts.Account";
const SESSION: &str = "tenancy.accounts.Session";

const OPEN_ACCOUNT: &str = "tenancy.accounts.OpenAccount";
const OPEN_SESSION: &str = "tenancy.accounts.OpenSession";
const HIT: &str = "tenancy.accounts.HitSession";
const CLOSE: &str = "tenancy.accounts.CloseAccount";
const BUMP: &str = "tenancy.accounts.BumpAccount";

const VIEWS: [(&str, &str, &str); 2] = [
    ("tenancy.accounts.AccountCard", ACCOUNT, "account_id"),
    ("tenancy.accounts.SessionCard", SESSION, "session_id"),
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-affects")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the adversary-affects fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input))
}

fn run(ir: &spindle_spec::EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    send(ir, store, command, input.clone())
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn held(store: &mut dyn Store, entity: &str, identity: &str) -> Option<Row> {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
        .map(|stored| stored.row)
}

/// The account `a1` with the session `s1`, and the account `a2`.
fn arrange(ir: &spindle_spec::EssIr, store: &mut dyn Store) {
    run(
        ir,
        store,
        OPEN_ACCOUNT,
        json!({"account_id": "a1", "level": 1}),
    );
    run(
        ir,
        store,
        OPEN_ACCOUNT,
        json!({"account_id": "a2", "level": 5}),
    );
    run(
        ir,
        store,
        OPEN_SESSION,
        json!({"session_id": "s1", "account_id": "a1", "hits": 0}),
    );
}

/// A store that, right after the kernel's first scan of `SESSION`, lets `other` (a second
/// handle on the same database) run `racing` to completion, then goes on.
struct Racing<'s> {
    inner: &'s mut dyn Store,
    other: &'s mut dyn Store,
    ir: &'s spindle_spec::EssIr,
    racing: Option<(&'static str, Value)>,
    raced: Option<Result<Executed, ExecuteError>>,
}

impl Store for Racing<'_> {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        let scan = self.inner.scan(entity)?;
        if entity == SESSION
            && let Some((command, input)) = self.racing.take()
        {
            self.raced = Some(send(self.ir, self.other, command, input));
        }
        Ok(scan)
    }
}

/// Closes `a1` while a second handle opens the session `s9` of `a1` between the kernel's scan of
/// the sessions and its commit. Either the close is refused as a conflict and `a1` and `s9` stand,
/// or `a1` is gone and so is every session of it.
fn a_session_opened_during_the_close_does_not_outlive_its_account(
    name: &str,
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    other: &mut dyn Store,
) {
    let mut racing = Racing {
        inner: store,
        other,
        ir,
        racing: Some((
            OPEN_SESSION,
            json!({"session_id": "s9", "account_id": "a1", "hits": 0}),
        )),
        raced: None,
    };
    let closed = send(ir, &mut racing, CLOSE, json!({"account_id": "a1"}));
    let raced = racing.raced.take().expect("the second writer ran");
    assert_eq!(
        raced
            .as_ref()
            .map(|executed| executed.outcome.as_str())
            .ok(),
        Some("opened"),
        "{name}: the second writer opened s9 while a1 still stood: {raced:?}"
    );
    let account = held(store, ACCOUNT, "a1");
    let session = held(store, SESSION, "s9");
    match closed {
        Err(ExecuteError::Store(StoreError::Conflict { .. })) => {
            assert!(account.is_some(), "{name}: a refused close leaves a1");
        }
        Ok(executed) => {
            assert_eq!(executed.outcome, "closed", "{name}");
            assert_eq!(account, None, "{name}: a1 is removed");
            assert_eq!(
                session, None,
                "{name}: the close of a1 landed after s9 of a1 was opened, and left s9 standing \
                 with no account: a removal-only write group carries no condition on the \
                 sessions it scanned"
            );
        }
        Err(other) => panic!("{name}: the close answers {other:?}"),
    }
}

#[test]
fn a_session_opened_during_the_close_does_not_outlive_its_account_on_sqlite() {
    let ir = ir();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("accounts.sqlite");
    let mut store = SqliteStore::open(&path).expect("SQLite opens");
    arrange(&ir, &mut store);
    let mut other = SqliteStore::open(&path).expect("a second handle on the file opens");
    a_session_opened_during_the_close_does_not_outlive_its_account(
        "sqlite", &ir, &mut store, &mut other,
    );
}

/// As above, with no session of `a1` held when the close scans: the close then selects none, and
/// the subject's removal is written on its own (`Store::remove`).
#[test]
fn a_session_opened_during_a_close_that_selected_none_does_not_outlive_its_account_on_sqlite() {
    let ir = ir();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("accounts.sqlite");
    let mut store = SqliteStore::open(&path).expect("SQLite opens");
    run(
        &ir,
        &mut store,
        OPEN_ACCOUNT,
        json!({"account_id": "a1", "level": 1}),
    );
    let mut other = SqliteStore::open(&path).expect("a second handle on the file opens");
    a_session_opened_during_the_close_does_not_outlive_its_account(
        "sqlite, none selected",
        &ir,
        &mut store,
        &mut other,
    );
}

/// A session of `a1` hit by a second handle between the scan and the commit: the close's write
/// of `s1` names a version `s1` no longer stands at, so nothing of the group lands, though the
/// removal of `a1` comes first in it.
fn a_close_refused_on_one_session_removes_nothing(
    name: &str,
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    other: &mut dyn Store,
) {
    run(
        ir,
        store,
        OPEN_SESSION,
        json!({"session_id": "s2", "account_id": "a1", "hits": 0}),
    );
    let mut racing = Racing {
        inner: store,
        other,
        ir,
        racing: Some((HIT, json!({"session_id": "s2"}))),
        raced: None,
    };
    let closed = send(ir, &mut racing, CLOSE, json!({"account_id": "a1"}));
    let raced = racing.raced.take().expect("the second writer ran");
    assert!(raced.is_ok(), "{name}: {raced:?}");
    assert!(
        closed.is_err(),
        "{name}: the close names a version of s2 it no longer holds: {closed:?}"
    );
    assert!(held(store, ACCOUNT, "a1").is_some(), "{name}: a1 stands");
    assert!(held(store, SESSION, "s1").is_some(), "{name}: s1 stands");
    assert_eq!(
        held(store, SESSION, "s2").and_then(|row| row.fields.get("hits").cloned()),
        Some(json!(1)),
        "{name}: s2 holds the second writer's hit"
    );
}

#[test]
fn a_close_refused_on_one_session_removes_nothing_on_sqlite() {
    let ir = ir();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("accounts.sqlite");
    let mut store = SqliteStore::open(&path).expect("SQLite opens");
    arrange(&ir, &mut store);
    let mut other = SqliteStore::open(&path).expect("a second handle on the file opens");
    a_close_refused_on_one_session_removes_nothing("sqlite", &ir, &mut store, &mut other);
}

#[cfg(feature = "postgres")]
pub(super) fn postgres_pair() -> (spindle_kernel::PostgresStore, spindle_kernel::PostgresStore) {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let database = spindle_kernel::Database::new(url);
    let letters: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .bytes()
        .take(12)
        .map(|byte| char::from(b'a' + byte % 26))
        .collect();
    let component = format!("adv-affects-{letters}");
    let open = || {
        spindle_kernel::PostgresStore::open(&database, &component, 2)
            .expect("the PostgreSQL store opens")
    };
    (open(), open())
}

#[cfg(feature = "postgres")]
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_session_opened_during_the_close_does_not_outlive_its_account_on_postgres() {
    let ir = ir();
    let (mut store, mut other) = postgres_pair();
    arrange(&ir, &mut store);
    a_session_opened_during_the_close_does_not_outlive_its_account(
        "postgres", &ir, &mut store, &mut other,
    );
}

#[cfg(feature = "postgres")]
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_close_refused_on_one_session_removes_nothing_on_postgres() {
    let ir = ir();
    let (mut store, mut other) = postgres_pair();
    arrange(&ir, &mut store);
    a_close_refused_on_one_session_removes_nothing("postgres", &ir, &mut store, &mut other);
}

/// Every value of `rows` read as ESS's [`Node`]: the interpreter writes an integral `Integer` as
/// `5.0` and the kernel as `5`, one number either way.
fn as_nodes(rows: Rows) -> Vec<Vec<BTreeMap<String, Node>>> {
    rows.into_iter()
        .map(|view| {
            view.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|(field, value)| {
                            let node = serde_json::from_value(value.clone())
                                .unwrap_or_else(|_| panic!("{field} holds a node, not {value}"));
                            (field, node)
                        })
                        .collect()
                })
                .collect()
        })
        .collect()
}

/// A row two entries select (`s1`: the first sets its hits to the amount, the second selects it as
/// it was, with no hits, and ends it), an entry over the subject's own entity whose filter reads
/// `subject.`, the subject's own `{expr: …}`, and a removal-only close.
#[test]
fn secondary_effects_with_expressions_agree_with_the_interpreter() {
    let ir = ir();
    let steps: Vec<(&str, Value)> = vec![
        (OPEN_ACCOUNT, json!({"account_id": "a1", "level": 1})),
        (OPEN_ACCOUNT, json!({"account_id": "a2", "level": 5})),
        (OPEN_ACCOUNT, json!({"account_id": "a3", "level": 0})),
        (
            OPEN_SESSION,
            json!({"session_id": "s1", "account_id": "a1", "hits": 0}),
        ),
        (
            OPEN_SESSION,
            json!({"session_id": "s2", "account_id": "a1", "hits": 3}),
        ),
        (
            OPEN_SESSION,
            json!({"session_id": "s3", "account_id": "a2", "hits": 0}),
        ),
        (
            OPEN_SESSION,
            json!({"session_id": "s4", "account_id": "a3", "hits": 7}),
        ),
        (
            OPEN_SESSION,
            json!({"session_id": "s5", "account_id": "a9", "hits": 7}),
        ),
        (BUMP, json!({"account_id": "a1", "by": 2})),
        (BUMP, json!({"account_id": "a3", "by": -4})),
        (HIT, json!({"session_id": "s4"})),
        (CLOSE, json!({"account_id": "a1"})),
        (CLOSE, json!({"account_id": "a2"})),
    ];
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let (answers, rows): (Vec<Answer>, Rows) =
        spindled_with(&VIEWS, &steps, &mut |store, command, input| {
            run(&ir, store, command, input)
        });
    assert_eq!(answers, ess_answers, "outcomes, errors and events agree");
    assert_eq!(
        as_nodes(rows),
        as_nodes(ess_rows),
        "the stored accounts and sessions agree with ESS's view rows"
    );
}
