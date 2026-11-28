//! Adversary of wave 33 U4 (`story:entity-invariant-over-stored-rows`): every invariant the new
//! model declares on an entity is judged over every stored row of it, as a row at rest is judged,
//! and each broken one is one misfit line naming the statement and how many rows break it.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use spindle_kernel::{Change, Fit, Misfit, Row, Shapes, SqliteStore, Store};

const BOOK: &str = "keep.shelf.Book";

const SYSTEM: &str = "format: ess/34
system: keep
version: v1
summary: Books kept in a durable store.
domains:
  - keep.shelf
components:
  - component: shelf
    summary: Keeps books.
    owns:
      domains:
        - keep.shelf
";

const BEFORE: &str = "domain: keep.shelf
summary: Books on shelves.
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: String}
    fields:
      - {name: title, type: String}
      - {name: weight, type: Integer}
      - {name: note, type: Optional<String>}
    lifecycle: {initial: Shelved, states: [Shelved], terminal: [Shelved]}
";

/// The book's last field, after which an `invariants:` list is spliced.
const LAST_FIELD: &str = "      - {name: note, type: Optional<String>}\n";

fn ir(domain: &str) -> spindle_spec::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/shelf.yaml".to_owned()],
        &[SYSTEM.to_owned(), domain.to_owned()],
    )
    .unwrap_or_else(|error| panic!("the specification compiles: {error}\n{domain}"))
}

/// [`BEFORE`] with `invariants` (YAML list items, already indented) declared on the book.
fn guarded(invariants: &str) -> spindle_spec::EssIr {
    let domain = BEFORE.replace(
        LAST_FIELD,
        &format!("{LAST_FIELD}    invariants:\n{invariants}"),
    );
    assert_ne!(domain, BEFORE);
    ir(&domain)
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// A store at `path` that recorded `recorded` and holds one book per entry of `rows`, each
/// `{"title": "Dune", "weight": 1}` overlaid with the entry.
fn holding(path: &Path, recorded: &Shapes, rows: &[Value]) -> SqliteStore {
    let mut store = SqliteStore::open(path).expect("the store opens");
    store.record(recorded).expect("the record is written");
    for (index, fields) in rows.iter().enumerate() {
        let mut row = json!({"title": "Dune", "weight": 1});
        row.as_object_mut()
            .expect("an object")
            .extend(fields.as_object().expect("an object").clone());
        let Value::Object(row) = row else {
            unreachable!()
        };
        let id = format!("b{index}");
        store
            .commit(&Change {
                entity: BOOK.to_owned(),
                identity: json!(id),
                expected: None,
                row: Row {
                    state: "Shelved".to_owned(),
                    fields: row.into_iter().collect::<BTreeMap<_, _>>(),
                },
                events: vec![spindle_kernel::Event {
                    name: "keep.shelf.Put".to_owned(),
                    payload: serde_json::Map::from_iter([("id".to_owned(), json!(id))]),
                }],
                scanned: Vec::new(),
            })
            .expect("the row is committed");
    }
    store
}

fn long(length: usize) -> String {
    "x".repeat(length)
}

/// Kept: the row count of an invariant misfit counts every row that breaks it, not whether one
/// does. The unit's cases break each invariant with exactly one row, so `*broken = 1` in place
/// of `*broken += 1` (`fit.rs`, `Held::of`) stays green there.
#[test]
fn two_stored_rows_breaking_one_invariant_misfit_with_two_rows() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &Shapes::of(&ir(BEFORE)),
        &[
            json!({"title": long(11)}),
            json!({}),
            json!({"title": long(30)}),
        ],
    );
    let after = Shapes::of(&guarded("      - 'title.count <= 10'\n"));
    let misfits = after
        .misfits(&Shapes::of(&ir(BEFORE)), &mut store)
        .expect("the store is read");
    assert_eq!(
        misfits,
        vec![Misfit::Invariant {
            entity: BOOK.to_owned(),
            invariant: "title.count <= 10".to_owned(),
            rows: 2,
        }]
    );
    assert!(misfits[0].to_string().ends_with(": 2"), "{}", misfits[0]);
}

/// Kept, a property over drawn rows (fixed seed): for each of two invariants, the misfit's row
/// count equals the number of stored rows `at_rest` refuses under a model declaring that one
/// invariant alone, and no misfit is returned where that number is 0.
#[test]
fn the_misfit_count_equals_the_rows_at_rest_refuses() {
    let mut seed: u64 = 0x5EED_0033_0004;
    let mut draw = |below: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % below
    };
    let rows: Vec<Value> = (0..40)
        .map(|_| {
            let mut row = json!({
                "title": long(usize::try_from(draw(16)).expect("small")),
                "weight": draw(5),
            });
            if draw(2) == 0 {
                row["note"] = json!(long(usize::try_from(draw(8)).expect("small")));
            }
            row
        })
        .collect();
    let recorded = Shapes::of(&ir(BEFORE));
    for statement in ["title.count <= 10", "note.count <= 4", "weight <= 2"] {
        let scratch = scratch();
        let mut store = holding(&scratch.path().join("state.sqlite"), &recorded, &rows);
        let model = guarded(&format!("      - '{statement}'\n"));
        let refused = store
            .scan(BOOK)
            .expect("the books are read")
            .into_iter()
            .filter(|(identity, stored)| {
                spindle_kernel::at_rest(&model, BOOK, identity, &stored.row).is_err()
            })
            .count();
        let misfits = Shapes::of(&model)
            .misfits(&recorded, &mut store)
            .expect("the store is read");
        let expected = if refused == 0 {
            Vec::new()
        } else {
            vec![Misfit::Invariant {
                entity: BOOK.to_owned(),
                invariant: statement.to_owned(),
                rows: refused,
            }]
        };
        assert_eq!(misfits, expected, "{statement}");
    }
}

/// Kept: rows of an entity the record does not name are still judged by the entity's invariants
/// (the invariant misfits are pushed before the `was` check in `Shapes::misfits`).
#[test]
fn rows_of_an_entity_the_record_lacks_are_judged() {
    let scratch = scratch();
    let empty = "domain: keep.shelf
summary: Books on shelves.
entities:
  - name: keep.shelf.Card
    identity: {name: id, type: String}
    fields:
      - {name: code, type: String}
    lifecycle: {initial: Filed, states: [Filed], terminal: [Filed]}
";
    let recorded = Shapes::of(&ir(empty));
    assert!(!recorded.entities.contains_key(BOOK));
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &recorded,
        &[json!({"title": long(11)})],
    );
    let after = Shapes::of(&guarded("      - 'title.count <= 10'\n"));
    assert_eq!(
        after
            .misfits(&recorded, &mut store)
            .expect("the store is read"),
        vec![Misfit::Invariant {
            entity: BOOK.to_owned(),
            invariant: "title.count <= 10".to_owned(),
            rows: 1,
        }]
    );
}

/// D2 A, one line per broken invariant: an invariant whose statement holds a line break (inside a
/// text literal; ESS reads a break between operands as part of a literal and refuses it) still
/// gives one line, as `run` prints each misfit on a line of its own
/// (`spindle-cli/src/database.rs`, `Misfitted`).
#[test]
fn an_invariant_holding_a_line_break_misfits_on_one_line() {
    let scratch = scratch();
    let recorded = Shapes::of(&ir(BEFORE));
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &recorded,
        &[json!({"title": "one\ntwo"})],
    );
    let model = guarded("      - \"title != 'one\\ntwo'\"\n");
    let mut misfits = Shapes::of(&model)
        .misfits(&recorded, &mut store)
        .expect("the store is read");
    assert_eq!(misfits.len(), 1, "{misfits:?}");
    let line = misfits.remove(0).to_string();
    assert_eq!(line.lines().count(), 1, "one line: {line:?}");
    assert!(line.ends_with(": 1"), "{line:?}");
}

/// A store whose rows all hold the invariant fits and records the new specification.
#[test]
fn a_row_at_the_bound_fits() {
    let scratch = scratch();
    let recorded = Shapes::of(&ir(BEFORE));
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &recorded,
        &[json!({"title": long(10)}), json!({"title": ""})],
    );
    let after = Shapes::of(&guarded("      - 'title.count <= 10'\n"));
    assert_eq!(
        store.fit(&after).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
}
