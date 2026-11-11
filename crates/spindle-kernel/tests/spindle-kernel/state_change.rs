//! `when_state_changes:` (story:state-change-conditions): a branch taken where its own move does,
//! or does not, change the state the subject holds immediately before selection, conjunctive with
//! its `when:`. The fixture `tests/fixtures/state-change` is a roster rebuilt from pushes that
//! re-report each member's state.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Executed, MemoryStore, NoExternals, SqliteStore, Store, execute};
use spindle_spec::EssIr;

const MEMBER: &str = "room.roster.Member";
const ADD: &str = "room.roster.AddMember";
const REPORT: &str = "room.roster.Report";

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/state-change")
}

fn ir() -> EssIr {
    spindle_spec::load(&root())
        .expect("the state-change fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn run(ir: &EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    execute(ir, store, &mut NoExternals, command, &object(input.clone()))
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn report(ir: &EssIr, store: &mut dyn Store, member: &str, state: &str, hold: bool) -> Executed {
    run(
        ir,
        store,
        REPORT,
        json!({"member_id": member, "state": state, "on_hold": hold}),
    )
}

fn add(ir: &EssIr, store: &mut dyn Store, member: &str) {
    let added = run(ir, store, ADD, json!({"member_id": member}));
    assert_eq!(added.outcome, "added");
}

/// The member's held state and `on_hold`, as the store reads them.
fn held(store: &mut dyn Store, member: &str) -> (String, Value) {
    let stored = store
        .load(MEMBER, &json!(member))
        .expect("the store reads")
        .expect("the member exists");
    (stored.row.state, stored.row.fields["on_hold"].clone())
}

fn events(executed: &Executed) -> Vec<&str> {
    executed
        .events
        .iter()
        .map(|event| event.name.as_str())
        .collect()
}

/// Each store a case runs on, by name: memory, and SQLite in a directory of its own.
fn stores() -> Vec<(&'static str, Box<dyn Store>, Option<tempfile::TempDir>)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let sqlite = SqliteStore::open(&dir.path().join("roster.sqlite")).expect("SQLite opens");
    vec![
        ("memory", Box::new(MemoryStore::default()), None),
        ("sqlite", Box::new(sqlite), Some(dir)),
    ]
}

#[test]
fn the_first_report_of_a_state_takes_the_changing_branch() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        add(&ir, store, "m1");

        let joined = report(&ir, store, "m1", "Added", false);
        assert_eq!(joined.outcome, "joined", "{name}: Pending is not Added");
        assert_eq!(events(&joined), ["room.roster.MemberJoined"], "{name}");
        assert_eq!(held(store, "m1").0, "Added", "{name}: the member moved");

        let left = report(&ir, store, "m1", "Gone", false);
        assert_eq!(left.outcome, "left", "{name}: Added is not Gone");
        assert_eq!(held(store, "m1").0, "Gone", "{name}");

        let back = report(&ir, store, "m1", "Added", false);
        assert_eq!(back.outcome, "joined", "{name}: Gone is not Added either");
        assert_eq!(held(store, "m1").0, "Added", "{name}");
    }
}

#[test]
fn a_repeated_report_falls_to_the_unchanged_branch() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        add(&ir, store, "m1");
        assert_eq!(report(&ir, store, "m1", "Added", false).outcome, "joined");

        let again = report(&ir, store, "m1", "Added", false);
        assert_eq!(
            again.outcome, "rejoined",
            "{name}: `when_state_changes: false` admits the state the move arrives at"
        );
        assert_eq!(events(&again), ["room.roster.MemberRejoined"], "{name}");
        assert_eq!(
            held(store, "m1").0,
            "Added",
            "{name}: the state is as it was"
        );

        // `left` admits every state but Gone; a member already Gone has no `false` branch for
        // Gone and falls to the default, which changes fields only.
        assert_eq!(report(&ir, store, "m1", "Gone", false).outcome, "left");
        let unchanged = report(&ir, store, "m1", "Gone", false);
        assert_eq!(unchanged.outcome, "refreshed", "{name}");
        assert_eq!(held(store, "m1").0, "Gone", "{name}: still Gone");
    }
}

/// Conjunctive with `when:`: the held state alone selects nothing. ESS reads the held state, then
/// the input guard (`interpret/execute/subject.rs:125`).
#[test]
fn the_state_change_holds_only_beside_its_input_guard() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        add(&ir, store, "m1");

        // Pending admits `joined` and `left` by their held state; the input names neither.
        let pending = report(&ir, store, "m1", "Pending", false);
        assert_eq!(pending.outcome, "refreshed", "{name}");
        assert_eq!(events(&pending), ["room.roster.MemberUpdated"], "{name}");
        assert_eq!(held(store, "m1").0, "Pending", "{name}: no move was taken");

        // `when: state == Gone` holds; the held state picks `left`, never `joined`.
        assert_eq!(report(&ir, store, "m1", "Gone", false).outcome, "left");
    }
}

/// The order of selection is ESS's precedence plan: a `when_state_changes:` branch answers among
/// the held-state branches (step 3), before an accepting `when:` branch (step 5) whose guard also
/// holds (`ess-domain/src/command/precedence.rs`, `place`). ESS refuses the model where the
/// declaration order says otherwise, so `paused` is declared after them.
#[test]
fn a_held_state_branch_answers_before_an_input_branch() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        add(&ir, store, "m1");

        let joined = report(&ir, store, "m1", "Added", true);
        assert_eq!(
            joined.outcome, "joined",
            "{name}: `paused`'s guard holds too"
        );
        let again = report(&ir, store, "m1", "Added", true);
        assert_eq!(again.outcome, "rejoined", "{name}");

        let paused = report(&ir, store, "m1", "Pending", true);
        assert_eq!(
            paused.outcome, "paused",
            "{name}: no held-state branch holds"
        );
        assert_eq!(
            held(store, "m1"),
            ("Added".to_owned(), json!(true)),
            "{name}"
        );
    }
}

/// With no prior state there is nothing to compare: a member nobody holds is answered by the
/// command's `unknown_instance:` branch before any held-state branch is read, as ESS's `held_rows`
/// answers it (`interpret/execute.rs`, `held_rows`).
#[test]
fn a_report_about_no_member_is_an_unknown_instance() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        let unknown = report(&ir, store, "m9", "Added", false);
        assert_eq!(unknown.outcome, "no-such-member", "{name}");
        assert_eq!(
            unknown.error.map(|error| error.name),
            Some("room.roster.NoSuchMember".to_owned()),
            "{name}"
        );
        assert!(unknown.events.is_empty(), "{name}");
        assert_eq!(
            store.load(MEMBER, &json!("m9")).expect("the store reads"),
            None,
            "{name}: nothing was created"
        );
    }
}

#[test]
fn state_change_reads_the_state_before_selection() {
    let ir = ir();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("roster.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        add(&ir, &mut store, "m1");
        assert_eq!(
            report(&ir, &mut store, "m1", "Added", false).outcome,
            "joined"
        );
    }

    let mut store = SqliteStore::open(&path).expect("SQLite reopens");
    assert_eq!(held(&mut store, "m1").0, "Added", "the move was persisted");
    let again = report(&ir, &mut store, "m1", "Added", false);
    assert_eq!(
        again.outcome, "rejoined",
        "the held state read after the restart is the persisted one"
    );
    let left = report(&ir, &mut store, "m1", "Gone", false);
    assert_eq!(left.outcome, "left");
    drop(store);

    let mut store = SqliteStore::open(&path).expect("SQLite reopens");
    assert_eq!(
        report(&ir, &mut store, "m1", "Gone", false).outcome,
        "refreshed"
    );
    assert_eq!(held(&mut store, "m1").0, "Gone");
}

/// The fixture's documents, with `edit` applied to the domain.
fn compiled(edit: impl FnOnce(String) -> String) -> Result<EssIr, spindle_spec::LoadError> {
    let read = |path: &str| std::fs::read_to_string(root().join(path)).expect("the fixture reads");
    let identities = ["system.yaml".to_owned(), "domains/roster.yaml".to_owned()];
    let texts = [read("system.yaml"), edit(read("domains/roster.yaml"))];
    spindle_spec::compile(&identities, &texts)
}

/// A creation and an update arrive at no state to compare against; ESS refuses the key on either
/// before any kernel sees the model (`ess-domain/src/command.rs`, the `when_state_changes`
/// conflict on a branch without `moves:`).
#[test]
fn a_branch_without_a_move_cannot_test_a_state_change() {
    assert!(
        compiled(|text| text).is_ok(),
        "the fixture as written compiles"
    );
    for (what, key) in [
        ("creation", "        creates: room.roster.Member\n"),
        ("update", "        updates: room.roster.Member\n"),
    ] {
        let refused = compiled(|text| {
            assert!(text.contains(key), "{what}: the fixture has {key:?}");
            text.replacen(key, &format!("{key}        when_state_changes: true\n"), 1)
        })
        .err()
        .unwrap_or_else(|| panic!("{what}: `when_state_changes:` beside no move compiles"));
        assert!(
            refused.to_string().contains("when_state_changes")
                || format!("{refused:?}").contains("when_state_changes"),
            "{what}: {refused:?}"
        );
    }
}
