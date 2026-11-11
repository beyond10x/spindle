//! Secondary effects (`affects:`, ess/16, ess/22, ess/23): after the subject's own effect, each
//! entry selects the rows of its entity whose filter holds, read from the store as it was before
//! the outcome, the subject itself excepted, and moves them (a row outside the move's `from`
//! states is skipped), removes them or updates them with the entry's `sets:`, entries applied in
//! declaration order and every row left in place checked at rest, as ESS's interpreter does
//! (`interpret::execute::set_effects`). Every row the command writes lands in one write group, or
//! none does.
//!
//! The fixture `tests/fixtures/notebooks` holds notebooks, their pages and their drafts.
//! `ArchiveNotebook` moves the notebook to `Archived`, moves every page of it from `Open` to
//! `Archived` with a note, and removes every draft of it. `PromotePages` marks the notebook
//! promoted, ranks its pages, stamps every page unranked before the outcome, and unpromotes every
//! other active notebook.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Removal, Row, Scan, SqliteStore,
    Store, StoreError, Stored, Write, execute,
};

use super::related_guards::{Answer, Rows, interpreted, spindled_with};

const NOTEBOOK: &str = "writing.notebooks.Notebook";
const PAGE: &str = "writing.notebooks.Page";
const DRAFT: &str = "writing.notebooks.Draft";

const OPEN: &str = "writing.notebooks.OpenNotebook";
const ADD_PAGE: &str = "writing.notebooks.AddPage";
const ARCHIVE_PAGE: &str = "writing.notebooks.ArchivePage";
const ADD_DRAFT: &str = "writing.notebooks.AddDraft";
const ARCHIVE: &str = "writing.notebooks.ArchiveNotebook";
const PROMOTE: &str = "writing.notebooks.PromotePages";

const NOTE: &str = "notebook archived";
const STAMP: &str = "was unranked";

const VIEWS: [(&str, &str, &str); 3] = [
    ("writing.notebooks.NotebookCard", NOTEBOOK, "notebook_id"),
    ("writing.notebooks.PageCard", PAGE, "page_id"),
    ("writing.notebooks.DraftCard", DRAFT, "draft_id"),
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notebooks")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the notebooks fixture compiles")
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

/// Each store a case runs on, by name: memory, and SQLite in a directory of its own.
fn stores() -> Vec<(&'static str, Box<dyn Store>, Option<tempfile::TempDir>)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let sqlite = SqliteStore::open(&dir.path().join("notebooks.sqlite")).expect("SQLite opens");
    vec![
        ("memory", Box::new(MemoryStore::default()), None),
        ("sqlite", Box::new(sqlite), Some(dir)),
    ]
}

fn row(store: &mut dyn Store, entity: &str, identity: &str) -> Option<Row> {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
        .map(|stored| stored.row)
}

fn held(store: &mut dyn Store, entity: &str, identity: &str) -> Row {
    row(store, entity, identity).unwrap_or_else(|| panic!("{entity} {identity} is held"))
}

/// The identities of every held row of `entity`, in the order the store scans them.
fn identities(store: &mut dyn Store, entity: &str) -> Vec<Value> {
    store
        .scan(entity)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, _)| identity)
        .collect()
}

/// Two notebooks, `n1` holding the pages `p1` and `p2` and the drafts `d1` and `d2`, `n2` holding
/// the page `p3` and the draft `d3`.
fn arrange(ir: &spindle_spec::EssIr, store: &mut dyn Store) {
    for step in arrangement() {
        run(ir, store, step.0, step.1);
    }
}

fn arrangement() -> Vec<(&'static str, Value)> {
    vec![
        (OPEN, json!({"notebook_id": "n1"})),
        (OPEN, json!({"notebook_id": "n2"})),
        (ADD_PAGE, json!({"page_id": "p1", "notebook_id": "n1"})),
        (ADD_PAGE, json!({"page_id": "p2", "notebook_id": "n1"})),
        (ADD_PAGE, json!({"page_id": "p3", "notebook_id": "n2"})),
        (ADD_DRAFT, json!({"draft_id": "d1", "notebook_id": "n1"})),
        (ADD_DRAFT, json!({"draft_id": "d2", "notebook_id": "n1"})),
        (ADD_DRAFT, json!({"draft_id": "d3", "notebook_id": "n2"})),
    ]
}

#[test]
fn an_affects_entry_moves_the_selected_rows() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let other = held(store, PAGE, "p3");

        let archived = run(&ir, store, ARCHIVE, json!({"notebook_id": "n1"}));

        assert_eq!(archived.outcome, "archived", "{name}");
        assert_eq!(archived.error, None, "{name}");
        assert_eq!(held(store, NOTEBOOK, "n1").state, "Archived", "{name}");
        for page in ["p1", "p2"] {
            let page = held(store, PAGE, page);
            assert_eq!(
                page.state, "Archived",
                "{name}: the notebook's open page moves"
            );
            assert_eq!(
                page.fields.get("archived_note"),
                Some(&json!(NOTE)),
                "{name}: the moved page carries the note"
            );
            assert_eq!(page.fields.get("rank"), Some(&json!(0)), "{name}");
        }
        assert_eq!(
            held(store, PAGE, "p3"),
            other,
            "{name}: another notebook's page is left as it was"
        );
        assert_eq!(
            archived
                .events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["writing.notebooks.NotebookArchived"],
            "{name}: the outcome publishes its own event"
        );
    }
}

#[test]
fn a_row_outside_from_is_skipped() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let archived = run(&ir, store, ARCHIVE_PAGE, json!({"page_id": "p1"}));
        assert_eq!(archived.outcome, "archived", "{name}");
        let before = store
            .load(PAGE, &json!("p1"))
            .expect("the store reads")
            .expect("p1 is held");

        let notebook = run(&ir, store, ARCHIVE, json!({"notebook_id": "n1"}));

        assert_eq!(notebook.outcome, "archived", "{name}");
        assert_eq!(
            store.load(PAGE, &json!("p1")).expect("the store reads"),
            Some(before),
            "{name}: an already archived page is neither written nor given the note"
        );
        assert_eq!(
            held(store, PAGE, "p2").fields.get("archived_note"),
            Some(&json!(NOTE)),
            "{name}: the open page beside it moves"
        );
    }
}

#[test]
fn a_deleting_entry_removes_the_selected_rows() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);

        let archived = run(&ir, store, ARCHIVE, json!({"notebook_id": "n1"}));

        assert_eq!(archived.outcome, "archived", "{name}");
        assert_eq!(row(store, DRAFT, "d1"), None, "{name}: d1 is gone");
        assert_eq!(row(store, DRAFT, "d2"), None, "{name}: d2 is gone");
        assert_eq!(
            identities(store, DRAFT),
            [json!("d3")],
            "{name}: the drafts' view lists none of the notebook's, and the other's stays"
        );

        let again = run(&ir, store, ARCHIVE, json!({"notebook_id": "n2"}));
        assert_eq!(again.outcome, "archived", "{name}");
        assert!(
            identities(store, DRAFT).is_empty(),
            "{name}: the drafts' view lists none"
        );
    }
}

#[test]
fn entries_apply_in_declaration_order_over_the_rows_before() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);

        let promoted = run(&ir, store, PROMOTE, json!({"notebook_id": "n1", "rank": 5}));

        assert_eq!(promoted.outcome, "promoted", "{name}");
        for page in ["p1", "p2"] {
            let page = held(store, PAGE, page);
            assert_eq!(
                page.fields.get("rank"),
                Some(&json!(5)),
                "{name}: the first entry ranks the notebook's pages"
            );
            assert_eq!(
                page.fields.get("stamp"),
                Some(&json!(STAMP)),
                "{name}: the second entry selects the page as it was before the outcome, \
                 unranked, though the first entry ranked it"
            );
        }
        let other = held(store, PAGE, "p3");
        assert_eq!(other.fields.get("rank"), Some(&json!(0)), "{name}");
        assert_eq!(other.fields.get("stamp"), Some(&json!(STAMP)), "{name}");
        assert_eq!(
            held(store, NOTEBOOK, "n1").fields.get("promoted"),
            Some(&json!(true)),
            "{name}: the subject is excepted from the entry over its own entity"
        );
        assert_eq!(
            held(store, NOTEBOOK, "n2").fields.get("promoted"),
            Some(&json!(false)),
            "{name}: every other active notebook is unpromoted"
        );
    }
}

#[test]
fn a_secondary_effect_breaking_an_invariant_commits_nothing() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        arrange(&ir, store);
        let before: Vec<Scan> = [NOTEBOOK, PAGE, DRAFT]
            .into_iter()
            .map(|entity| store.scan(entity).expect("the store scans"))
            .collect();

        let refused = send(
            &ir,
            store,
            PROMOTE,
            json!({"notebook_id": "n1", "rank": 11}),
        );

        assert!(
            matches!(refused, Err(ExecuteError::BrokenInvariant { .. })),
            "{name}: a page ranked 11 breaks `rank <= 10`: {refused:?}"
        );
        let after: Vec<Scan> = [NOTEBOOK, PAGE, DRAFT]
            .into_iter()
            .map(|entity| store.scan(entity).expect("the store scans"))
            .collect();
        assert_eq!(
            after, before,
            "{name}: neither the subject nor any row of any entry was written"
        );
    }
}

/// A store that lands `landed` on its inner store just before the first group of writes: another
/// writer adding a row between the outcome's selection and its commit.
struct Interleaved {
    inner: MemoryStore,
    landed: Option<Change>,
}

impl Store for Interleaved {
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
        if let Some(landed) = self.landed.take() {
            self.inner.commit(&landed)?;
        }
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.inner.scan(entity)
    }
}

#[test]
fn a_row_added_after_the_selection_refuses_the_command() {
    let ir = ir();
    let mut store = Interleaved {
        inner: MemoryStore::default(),
        landed: None,
    };
    arrange(&ir, &mut store);
    store.landed = Some(Change {
        entity: DRAFT.to_owned(),
        identity: json!("d4"),
        expected: None,
        row: Row {
            state: "Open".to_owned(),
            fields: BTreeMap::from([("notebook_id".to_owned(), json!("n1"))]),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    });

    let refused = send(&ir, &mut store, ARCHIVE, json!({"notebook_id": "n1"}));

    assert!(
        matches!(
            refused,
            Err(ExecuteError::Store(StoreError::Conflict { .. }))
        ),
        "a draft of the notebook added after the selection would escape its removal: {refused:?}"
    );
    assert_eq!(held(&mut store, NOTEBOOK, "n1").state, "Active");
    assert_eq!(
        identities(&mut store, DRAFT),
        [json!("d1"), json!("d2"), json!("d3"), json!("d4")],
        "nothing the command decided was written"
    );
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

#[test]
fn secondary_effects_agree_with_the_interpreter() {
    let ir = ir();
    let mut steps = arrangement();
    steps.extend([
        (ADD_PAGE, json!({"page_id": "p4", "notebook_id": "n2"})),
        (ARCHIVE_PAGE, json!({"page_id": "p4"})),
        (PROMOTE, json!({"notebook_id": "n1", "rank": 5})),
        (PROMOTE, json!({"notebook_id": "n2", "rank": 3})),
        (ARCHIVE, json!({"notebook_id": "n1"})),
        (ARCHIVE, json!({"notebook_id": "n1"})),
        (OPEN, json!({"notebook_id": "n3"})),
        (ARCHIVE, json!({"notebook_id": "n3"})),
        (ARCHIVE, json!({"notebook_id": "n2"})),
    ]);
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes[8..],
        [
            "added",
            "archived",
            "promoted",
            "promoted",
            "archived",
            "not-active",
            "opened",
            "archived",
            "archived",
        ],
        "the steps move, update and remove rows, skip one outside `from` and select none"
    );
    let (answers, rows): (Vec<Answer>, Rows) =
        spindled_with(&VIEWS, &steps, &mut |store, command, input| {
            run(&ir, store, command, input)
        });
    assert_eq!(answers, ess_answers, "outcomes, errors and events agree");
    assert_eq!(
        as_nodes(rows),
        as_nodes(ess_rows),
        "the stored notebooks, pages and drafts agree with ESS's view rows"
    );
}

#[cfg(feature = "postgres")]
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn secondary_effects_land_in_one_write_group_on_postgres() {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let component = format!("affects-{}", uuid::Uuid::new_v4().simple());
    let mut store =
        spindle_kernel::PostgresStore::open(&spindle_kernel::Database::new(url), &component, 1)
            .expect("the PostgreSQL store opens");
    let ir = ir();
    arrange(&ir, &mut store);

    let archived = run(&ir, &mut store, ARCHIVE, json!({"notebook_id": "n1"}));

    assert_eq!(archived.outcome, "archived");
    assert_eq!(held(&mut store, NOTEBOOK, "n1").state, "Archived");
    assert_eq!(held(&mut store, PAGE, "p1").state, "Archived");
    assert_eq!(held(&mut store, PAGE, "p2").state, "Archived");
    assert_eq!(held(&mut store, PAGE, "p3").state, "Open");
    assert_eq!(identities(&mut store, DRAFT), [json!("d3")]);

    let refused = send(
        &ir,
        &mut store,
        PROMOTE,
        json!({"notebook_id": "n2", "rank": 11}),
    );
    assert!(
        matches!(refused, Err(ExecuteError::BrokenInvariant { .. })),
        "{refused:?}"
    );
    assert_eq!(
        held(&mut store, NOTEBOOK, "n2").fields.get("promoted"),
        None
    );
}
