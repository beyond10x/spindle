//! `story:spec-change-over-stored-rows`: a durable store records the specification it was written
//! under, and a later specification fits its rows unless it cannot read one: the misfit rule
//! (D1 A, D4 A), on a SQLite store.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use spindle_kernel::{Change, Fit, Misfit, Row, Shapes, SqliteStore, Store};

const BOOK: &str = "keep.shelf.Book";
const SHELF: &str = "keep.shelf.Shelf";

const SYSTEM: &str = "format: ess/34
system: keep
version: v1
summary: Books and shelves kept in a durable store.
domains:
  - keep.shelf
components:
  - component: shelf
    summary: Keeps books and shelves.
    owns:
      domains:
        - keep.shelf
";

const BEFORE: &str = "domain: keep.shelf
summary: Books on shelves.
types:
  - name: keep.shelf.BookId
    kind: newtype
    of: String
  - name: keep.shelf.Weight
    kind: newtype
    of: Integer
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: keep.shelf.BookId}
    fields:
      - {name: title, type: String}
      - {name: weight, type: keep.shelf.Weight}
      - {name: note, type: Optional<String>}
    lifecycle: {initial: Shelved, states: [Shelved], terminal: [Shelved]}
  - name: keep.shelf.Shelf
    identity: {name: id, type: String}
    fields:
      - {name: label, type: String}
    lifecycle: {initial: Open, states: [Open], terminal: [Open]}
";

/// The model of `domain`, the one domain of the system.
fn ir(domain: &str) -> spindle_spec::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/shelf.yaml".to_owned()],
        &[SYSTEM.to_owned(), domain.to_owned()],
    )
    .unwrap_or_else(|error| panic!("the specification compiles: {error}\n{domain}"))
}

/// [`BEFORE`] with `from` replaced by `to`, which must change it.
fn changed(from: &str, to: &str) -> Shapes {
    let domain = BEFORE.replace(from, to);
    assert_ne!(domain, BEFORE, "{from:?} is in the specification");
    Shapes::of(&ir(&domain))
}

fn before() -> Shapes {
    Shapes::of(&ir(BEFORE))
}

fn open(path: &Path) -> SqliteStore {
    SqliteStore::open(path).expect("the store opens")
}

fn put(store: &mut SqliteStore, entity: &str, id: &str, state: &str, fields: Value) {
    let Value::Object(fields) = fields else {
        panic!("fields are an object")
    };
    store
        .commit(&Change {
            entity: entity.to_owned(),
            identity: json!(id),
            expected: None,
            row: Row {
                state: state.to_owned(),
                fields: fields.into_iter().collect::<BTreeMap<_, _>>(),
            },
            events: vec![spindle_kernel::Event {
                name: "keep.shelf.Put".to_owned(),
                payload: serde_json::Map::from_iter([("id".to_owned(), json!(id))]),
            }],
            scanned: Vec::new(),
        })
        .expect("the row is committed");
}

/// A store at `path` that recorded [`BEFORE`] and holds one book with a title and a weight.
fn written(path: &Path) -> SqliteStore {
    let mut store = open(path);
    assert_eq!(
        store.fit(&before()).expect("the store is checked"),
        Fit::Fits { unrecorded: false }
    );
    put(
        &mut store,
        BOOK,
        "b1",
        "Shelved",
        json!({"title": "Dune", "weight": 3}),
    );
    store
}

/// The misfits `after` answers on `store`, which keeps its record of [`BEFORE`].
fn misfits(store: &mut SqliteStore, after: &Shapes) -> Vec<Misfit> {
    match store.fit(after).expect("the store is checked") {
        Fit::Misfits(misfits) => {
            assert_eq!(
                store.recorded().expect("the record reads"),
                Some(before()),
                "a misfit leaves the record as it was"
            );
            misfits
        }
        other => panic!("the change misfits, not {other:?}"),
    }
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

#[test]
fn the_store_records_the_specification_it_was_written_under() {
    let scratch = scratch();
    let path = scratch.path().join("state.sqlite");
    drop(written(&path));
    let mut store = open(&path);
    assert_eq!(store.recorded().expect("the record reads"), Some(before()));
    assert_eq!(
        store
            .recorded()
            .expect("the record reads")
            .map(|shapes| shapes.digest),
        Some(
            ess_conformance::SuiteProvenance::of(&ir(BEFORE))
                .spec_digest
                .to_string()
        ),
        "the record holds the model's digest"
    );
    assert_eq!(store.fit(&before()).expect("checked"), Fit::Same);
}

#[test]
fn an_optional_field_added_fits_stored_rows() {
    let scratch = scratch();
    let mut store = written(&scratch.path().join("state.sqlite"));
    let after = changed(
        "      - {name: note, type: Optional<String>}\n",
        "      - {name: note, type: Optional<String>}\n      - {name: shelfmark, type: Optional<String>}\n",
    );
    assert_eq!(
        store.fit(&after).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    assert_eq!(store.recorded().expect("the record reads"), Some(after));
}

#[test]
fn a_removed_field_misfits_only_where_rows_hold_it() {
    let scratch = scratch();
    let mut store = written(&scratch.path().join("state.sqlite"));
    // The book holds a title and no note; no shelf holds a label.
    let after = changed("      - {name: title, type: String}\n", "")
        .entities
        .into_iter()
        .map(|(entity, mut shape)| {
            shape.fields.remove("note");
            shape.fields.remove("label");
            (entity, shape)
        })
        .collect();
    let after = Shapes {
        digest: "another".to_owned(),
        entities: after,
    };
    assert_eq!(
        misfits(&mut store, &after),
        vec![Misfit::Removed {
            entity: BOOK.to_owned(),
            field: "title".to_owned(),
            rows: 1,
        }]
    );
}

#[test]
fn a_retyped_field_misfits_through_a_named_type() {
    let scratch = scratch();
    let mut store = written(&scratch.path().join("state.sqlite"));
    let after = changed(
        "  - name: keep.shelf.Weight\n    kind: newtype\n    of: Integer\n",
        "  - name: keep.shelf.Weight\n    kind: newtype\n    of: String\n",
    );
    let misfits = misfits(&mut store, &after);
    assert_eq!(
        misfits,
        vec![Misfit::Retyped {
            entity: BOOK.to_owned(),
            field: "weight".to_owned(),
            rows: 1,
        }]
    );
    let line = misfits[0].to_string();
    assert!(
        line.contains(BOOK) && line.contains("weight"),
        "the misfit names the entity and the field: {line}"
    );
}

#[test]
fn a_required_field_added_misfits_stored_rows() {
    let scratch = scratch();
    let mut store = written(&scratch.path().join("state.sqlite"));
    let after = changed(
        "      - {name: note, type: Optional<String>}\n",
        "      - {name: note, type: Optional<String>}\n      - {name: isbn, type: String}\n",
    );
    assert_eq!(
        misfits(&mut store, &after),
        vec![Misfit::Required {
            entity: BOOK.to_owned(),
            field: "isbn".to_owned(),
            rows: 1,
        }]
    );
}

#[test]
fn a_removed_entity_holding_rows_misfits() {
    let scratch = scratch();
    let mut store = written(&scratch.path().join("state.sqlite"));
    put(&mut store, SHELF, "s1", "Open", json!({"label": "A"}));
    let shelf = BEFORE
        .find("  - name: keep.shelf.Shelf\n")
        .expect("the shelf is declared");
    let after = Shapes::of(&ir(&BEFORE[..shelf]));
    assert_eq!(
        misfits(&mut store, &after),
        vec![Misfit::Entity {
            entity: SHELF.to_owned(),
            rows: 1,
        }]
    );
}

/// The store was written under [`BEFORE`] with a second state, `Lent`, a book resting in it; the
/// state is gone from [`BEFORE`]. (The shapes are the rule's input: ESS refuses a state no command
/// reaches, so the earlier one is [`BEFORE`]'s with the state added.)
#[test]
fn a_removed_state_holding_rows_misfits() {
    let scratch = scratch();
    let mut store = open(&scratch.path().join("state.sqlite"));
    let mut lending = before();
    lending.digest = "lending".to_owned();
    lending
        .entities
        .get_mut(BOOK)
        .expect("the book is shaped")
        .states
        .insert("Lent".to_owned());
    assert_eq!(
        store.fit(&lending).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    put(
        &mut store,
        BOOK,
        "b1",
        "Shelved",
        json!({"title": "Dune", "weight": 3}),
    );
    put(
        &mut store,
        BOOK,
        "b2",
        "Lent",
        json!({"title": "Emma", "weight": 2}),
    );
    assert_eq!(
        store.fit(&before()).expect("checked"),
        Fit::Misfits(vec![Misfit::State {
            entity: BOOK.to_owned(),
            state: "Lent".to_owned(),
            rows: 1,
        }])
    );
    assert_eq!(
        store.recorded().expect("the record reads"),
        Some(lending),
        "a misfit leaves the record as it was"
    );
}

#[test]
fn an_empty_store_fits_any_specification() {
    let scratch = scratch();
    let mut store = open(&scratch.path().join("state.sqlite"));
    assert_eq!(
        store.fit(&before()).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    let shelf = BEFORE
        .find("  - name: keep.shelf.Shelf\n")
        .expect("the shelf is declared");
    let after = Shapes::of(&ir(&BEFORE[..shelf]
        .replace("      - {name: title, type: String}\n", "")
        .replace("of: Integer", "of: String")));
    assert_eq!(
        store.fit(&after).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    assert_eq!(store.recorded().expect("the record reads"), Some(after));
}

#[test]
fn a_store_without_a_record_starts_and_records() {
    let scratch = scratch();
    let path = scratch.path().join("state.sqlite");
    let mut store = open(&path);
    put(
        &mut store,
        BOOK,
        "b1",
        "Shelved",
        json!({"title": "Dune", "weight": 3}),
    );
    assert_eq!(store.recorded().expect("the record reads"), None);
    assert_eq!(
        store.fit(&before()).expect("checked"),
        Fit::Fits { unrecorded: true }
    );
    drop(store);
    let mut store = open(&path);
    assert_eq!(store.recorded().expect("the record reads"), Some(before()));
    assert_eq!(store.fit(&before()).expect("checked"), Fit::Same);
}

#[test]
fn the_record_is_not_a_published_event() {
    let scratch = scratch();
    let mut store = open(&scratch.path().join("state.sqlite"));
    store.fit(&before()).expect("checked");
    assert!(
        store
            .events_after(0, usize::MAX)
            .expect("the events read")
            .is_empty(),
        "the record alone reads back as no event"
    );
    put(
        &mut store,
        BOOK,
        "b1",
        "Shelved",
        json!({"title": "Dune", "weight": 3}),
    );
    let after = changed(
        "      - {name: note, type: Optional<String>}\n",
        "      - {name: note, type: Optional<String>}\n      - {name: shelfmark, type: Optional<String>}\n",
    );
    assert_eq!(
        store.fit(&after).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    let events: Vec<String> = store
        .events_after(0, usize::MAX)
        .expect("the events read")
        .into_iter()
        .map(|committed| committed.event.name)
        .collect();
    assert_eq!(events, vec!["keep.shelf.Put".to_owned()]);
}
