//! Adversary of wave 31 U5 (`story:spec-change-over-stored-rows`): the misfit rule read against
//! its own words. A change misfits only where the new specification cannot read a stored row
//! (`Shapes` in `src/fit.rs`: "a field that becomes optional keeps its rows", "a non-optional
//! field added, or made required, while rows lack it"), so a change every stored row still reads
//! fits, and one a stored row cannot be read under misfits, naming the entity and the field.

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
types:
  - name: keep.shelf.BookId
    kind: newtype
    of: String
  - name: keep.shelf.Genre
    kind: enum
    variants: [Fiction, Poetry]
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: keep.shelf.BookId}
    fields:
      - {name: title, type: String}
      - {name: note, type: Optional<String>}
      - {name: genre, type: keep.shelf.Genre}
    lifecycle: {initial: Shelved, states: [Shelved], terminal: [Shelved]}
";

fn ir(domain: &str) -> spindle_spec::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/shelf.yaml".to_owned()],
        &[SYSTEM.to_owned(), domain.to_owned()],
    )
    .unwrap_or_else(|error| panic!("the specification compiles: {error}\n{domain}"))
}

fn before() -> Shapes {
    Shapes::of(&ir(BEFORE))
}

/// [`BEFORE`] with `from` replaced by `to`, which must change it.
fn changed(from: &str, to: &str) -> Shapes {
    let domain = BEFORE.replace(from, to);
    assert_ne!(domain, BEFORE, "{from:?} is in the specification");
    Shapes::of(&ir(&domain))
}

fn put(store: &mut SqliteStore, id: &str, fields: Value) {
    let Value::Object(fields) = fields else {
        panic!("fields are an object")
    };
    store
        .commit(&Change {
            entity: BOOK.to_owned(),
            identity: json!(id),
            expected: None,
            row: Row {
                state: "Shelved".to_owned(),
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

/// A store at `path` that recorded [`BEFORE`] and holds each of `rows`, by identity.
fn written(path: &Path, rows: &[(&str, Value)]) -> SqliteStore {
    let mut store = SqliteStore::open(path).expect("the store opens");
    assert_eq!(
        store.fit(&before()).expect("the store is checked"),
        Fit::Fits { unrecorded: false }
    );
    for (id, fields) in rows {
        put(&mut store, id, fields.clone());
    }
    store
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

fn fits(store: &mut SqliteStore, after: &Shapes) {
    assert_eq!(
        store.fit(after).expect("checked"),
        Fit::Fits { unrecorded: false },
        "every stored row reads under the new specification"
    );
    assert_eq!(
        store.recorded().expect("the record reads").as_ref(),
        Some(after)
    );
}

fn misfits(store: &mut SqliteStore, after: &Shapes, expected: &[Misfit]) {
    assert_eq!(
        store.fit(after).expect("checked"),
        Fit::Misfits(expected.to_vec())
    );
    assert_eq!(
        store.recorded().expect("the record reads"),
        Some(before()),
        "a misfit leaves the record as it was"
    );
}

/// `src/fit.rs` says a non-optional field "made required, while rows lack it" misfits: made
/// required while every stored row holds a value, it fits. `reads` compares the required type
/// with the recorded `Optional<..>` and answers `Retyped` for every row holding the field.
#[test]
fn a_field_made_required_that_every_row_holds_fits() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[(
            "b1",
            json!({"title": "Dune", "note": "signed", "genre": "Fiction"}),
        )],
    );
    fits(
        &mut store,
        &changed(
            "{name: note, type: Optional<String>}",
            "{name: note, type: String}",
        ),
    );
}

/// The same change over a row that lacks the field: one `Required` misfit, and no `Retyped`.
#[test]
fn a_field_made_required_misfits_where_a_row_lacks_it() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"title": "Dune", "genre": "Fiction"}))],
    );
    misfits(
        &mut store,
        &changed(
            "{name: note, type: Optional<String>}",
            "{name: note, type: String}",
        ),
        &[Misfit::Required {
            entity: BOOK.to_owned(),
            field: "note".to_owned(),
            rows: 1,
        }],
    );
}

/// "a field that becomes optional keeps its rows".
#[test]
fn a_field_made_optional_fits_stored_rows() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"title": "Dune", "genre": "Fiction"}))],
    );
    fits(
        &mut store,
        &changed(
            "{name: title, type: String}",
            "{name: title, type: Optional<String>}",
        ),
    );
}

/// A removed field every row holds as `null` only: no stored row holds a value of it.
#[test]
fn a_removed_field_held_only_as_null_fits() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[(
            "b1",
            json!({"title": "Dune", "note": null, "genre": "Fiction"}),
        )],
    );
    fits(
        &mut store,
        &changed("      - {name: note, type: Optional<String>}\n", ""),
    );
}

/// A renamed field is a field removed while rows hold it and a required field the rows lack.
#[test]
fn a_renamed_field_misfits_as_removed_and_required() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[
            ("b1", json!({"title": "Dune", "genre": "Fiction"})),
            ("b2", json!({"title": "Emma", "genre": "Fiction"})),
        ],
    );
    misfits(
        &mut store,
        &changed(
            "{name: title, type: String}",
            "{name: heading, type: String}",
        ),
        &[
            Misfit::Removed {
                entity: BOOK.to_owned(),
                field: "title".to_owned(),
                rows: 2,
            },
            Misfit::Required {
                entity: BOOK.to_owned(),
                field: "heading".to_owned(),
                rows: 2,
            },
        ],
    );
}

/// An enum value removed while a row holds the field: the rows cannot all be read.
#[test]
fn an_enum_value_removed_misfits_rows_holding_the_field() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[
            ("b1", json!({"title": "Dune", "genre": "Fiction"})),
            ("b2", json!({"title": "Odes", "genre": "Poetry"})),
        ],
    );
    misfits(
        &mut store,
        &changed("variants: [Fiction, Poetry]", "variants: [Fiction]"),
        &[Misfit::Retyped {
            entity: BOOK.to_owned(),
            field: "genre".to_owned(),
            rows: 2,
        }],
    );
}

/// An enum value added: every stored value is still one of the enum's values, so every row reads.
/// `structure` writes an enum as its set of values and `reads` compares by equality, so the
/// change answers `Retyped` for every row holding the field.
#[test]
fn an_enum_value_added_fits_stored_rows() {
    let scratch = scratch();
    let mut store = written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"title": "Dune", "genre": "Fiction"}))],
    );
    fits(
        &mut store,
        &changed(
            "variants: [Fiction, Poetry]",
            "variants: [Fiction, Poetry, Drama]",
        ),
    );
}

// Round 2: the correction reads each stored value of a changed field under its new type
// (`typed::validate`). Nested types, an optional enum, a null under a field made required, and
// newtype constraints loosened, tightened and left alone.

const NESTED: &str = "domain: keep.shelf
summary: Books on shelves, tagged and rated.
types:
  - name: keep.shelf.BookId
    kind: newtype
    of: String
  - name: keep.shelf.Genre
    kind: enum
    variants: [Fiction, Poetry]
  - name: keep.shelf.Tag
    kind: struct
    fields:
      - {name: genre, type: keep.shelf.Genre}
      - {name: aside, type: Optional<String>}
  - name: keep.shelf.Code
    kind: newtype
    of: String
    prefix: c_
    invariants:
      - 'value.count <= 8'
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: keep.shelf.BookId}
    fields:
      - {name: title, type: String}
      - {name: tags, type: List<keep.shelf.Tag>}
      - {name: ratings, type: 'Map<String, keep.shelf.Genre>'}
      - {name: mood, type: Optional<keep.shelf.Genre>}
      - {name: code, type: keep.shelf.Code}
      - {name: note, type: Optional<String>}
    lifecycle: {initial: Shelved, states: [Shelved], terminal: [Shelved]}
";

fn nested() -> Shapes {
    Shapes::of(&ir(NESTED))
}

/// [`NESTED`] with each `(from, to)` replaced, each of which must change it.
fn nested_changed(edits: &[(&str, &str)]) -> Shapes {
    let mut domain = NESTED.to_owned();
    for (from, to) in edits {
        let edited = domain.replace(from, to);
        assert_ne!(edited, domain, "{from:?} is in the specification");
        domain = edited;
    }
    Shapes::of(&ir(&domain))
}

/// A book holding `fields` beside a title, no tags, no ratings and the code `c_1`, unless
/// `fields` sets them.
fn book(fields: Value) -> Value {
    let mut book = json!({"title": "Dune", "tags": [], "ratings": {}, "code": "c_1"});
    if let (Some(book), Value::Object(fields)) = (book.as_object_mut(), fields) {
        book.extend(fields);
    }
    book
}

/// A store at `path` that recorded [`NESTED`] and holds each of `rows`, by identity.
fn nested_written(path: &Path, rows: &[(&str, Value)]) -> SqliteStore {
    let mut store = SqliteStore::open(path).expect("the store opens");
    assert_eq!(
        store.fit(&nested()).expect("the store is checked"),
        Fit::Fits { unrecorded: false }
    );
    for (id, fields) in rows {
        put(&mut store, id, book(fields.clone()));
    }
    store
}

fn nested_misfits(store: &mut SqliteStore, after: &Shapes, expected: &[Misfit]) {
    assert_eq!(
        store.fit(after).expect("checked"),
        Fit::Misfits(expected.to_vec())
    );
    assert_eq!(
        store.recorded().expect("the record reads"),
        Some(nested()),
        "a misfit leaves the record as it was"
    );
}

fn retyped(field: &str, rows: usize) -> Misfit {
    Misfit::Retyped {
        entity: BOOK.to_owned(),
        field: field.to_owned(),
        rows,
    }
}

const WIDENED: (&str, &str) = (
    "variants: [Fiction, Poetry]",
    "variants: [Fiction, Poetry, Drama]",
);
const NARROWED: (&str, &str) = ("variants: [Fiction, Poetry]", "variants: [Fiction]");

#[test]
fn r2_an_enum_widened_inside_a_struct_inside_a_list_and_a_map_fits() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[(
            "b1",
            json!({"tags": [{"genre": "Poetry", "aside": "x"}], "ratings": {"a": "Fiction"}}),
        )],
    );
    fits(&mut store, &nested_changed(&[WIDENED]));
}

#[test]
fn r2_an_enum_narrowed_misfits_each_field_where_a_nested_value_uses_the_value() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[
            (
                "b1",
                json!({"tags": [{"genre": "Fiction"}, {"genre": "Poetry"}]}),
            ),
            ("b2", json!({"ratings": {"a": "Fiction", "b": "Poetry"}})),
            ("b3", json!({"mood": "Fiction"})),
        ],
    );
    // Every row holds tags and ratings (empty or not); one value of each is Poetry. No mood is.
    nested_misfits(
        &mut store,
        &nested_changed(&[NARROWED]),
        &[retyped("ratings", 3), retyped("tags", 3)],
    );
}

#[test]
fn r2_an_enum_narrowed_where_no_stored_value_uses_the_value_fits() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[
            (
                "b1",
                json!({"tags": [{"genre": "Fiction"}], "mood": "Fiction"}),
            ),
            ("b2", json!({"ratings": {"a": "Fiction"}, "mood": null})),
        ],
    );
    fits(&mut store, &nested_changed(&[NARROWED]));
}

#[test]
fn r2_an_optional_enum_narrowed_misfits_only_through_a_value_not_a_null() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[
            ("b1", json!({"mood": "Poetry"})),
            ("b2", json!({"mood": null})),
        ],
    );
    nested_misfits(
        &mut store,
        &nested_changed(&[NARROWED]),
        &[retyped("mood", 1)],
    );
}

#[test]
fn r2_a_null_held_under_a_field_made_required_misfits_as_required_alone() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[
            ("b1", json!({"mood": "Fiction"})),
            ("b2", json!({"mood": null})),
            ("b3", json!({})),
        ],
    );
    nested_misfits(
        &mut store,
        &nested_changed(&[(
            "{name: mood, type: Optional<keep.shelf.Genre>}",
            "{name: mood, type: keep.shelf.Genre}",
        )]),
        &[Misfit::Required {
            entity: BOOK.to_owned(),
            field: "mood".to_owned(),
            rows: 2,
        }],
    );
}

#[test]
fn r2_a_field_made_required_and_its_enum_widened_fits_where_every_row_holds_it() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"mood": "Fiction"}))],
    );
    fits(
        &mut store,
        &nested_changed(&[
            WIDENED,
            (
                "{name: mood, type: Optional<keep.shelf.Genre>}",
                "{name: mood, type: keep.shelf.Genre}",
            ),
        ]),
    );
}

#[test]
fn r2_a_struct_member_removed_misfits_where_a_stored_tag_holds_it() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"tags": [{"genre": "Fiction", "aside": "x"}]}))],
    );
    nested_misfits(
        &mut store,
        &nested_changed(&[("      - {name: aside, type: Optional<String>}\n", "")]),
        &[retyped("tags", 1)],
    );
}

#[test]
fn r2_a_loosened_newtype_constraint_fits() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"code": "c_1234"}))],
    );
    fits(
        &mut store,
        &nested_changed(&[
            ("    prefix: c_\n", ""),
            ("'value.count <= 8'", "'value.count <= 16'"),
        ]),
    );
}

#[test]
fn r2_a_tightened_newtype_constraint_no_stored_value_breaks_fits() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"code": "c_12"}))],
    );
    fits(
        &mut store,
        &nested_changed(&[("'value.count <= 8'", "'value.count <= 4'")]),
    );
}

#[test]
fn r2_a_tightened_newtype_prefix_a_stored_value_breaks_misfits() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"code": "c_12"}))],
    );
    nested_misfits(
        &mut store,
        &nested_changed(&[("    prefix: c_\n", "    prefix: cd_\n")]),
        &[retyped("code", 1)],
    );
}

#[test]
fn r2_an_unchanged_constrained_type_does_not_misfit_when_another_field_changes() {
    let scratch = scratch();
    let mut store = nested_written(
        &scratch.path().join("state.sqlite"),
        &[("b1", json!({"code": "c_12345", "note": "n"}))],
    );
    fits(
        &mut store,
        &nested_changed(&[(
            "      - {name: note, type: Optional<String>}\n",
            "      - {name: note, type: Optional<String>}\n      - {name: shelfmark, type: Optional<String>}\n",
        )]),
    );
}

/// A struct that ignored undeclared members (`undeclared_fields: ignored`) stored a tag holding
/// one; closed again, the kernel refuses that value (`typed::validate`), so the rows holding it
/// misfit. The structure written for a struct leaves the setting out, so nothing is read again.
#[test]
fn r2_a_struct_closed_to_undeclared_members_misfits_where_a_stored_value_holds_one() {
    let scratch = scratch();
    let open = NESTED.replace(
        "    kind: struct\n",
        "    kind: struct\n    undeclared_fields: ignored\n",
    );
    assert_ne!(open, NESTED);
    let recorded = Shapes::of(&ir(&open));
    let mut store = SqliteStore::open(&scratch.path().join("state.sqlite")).expect("opens");
    assert_eq!(
        store.fit(&recorded).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    put(
        &mut store,
        "b1",
        book(json!({"tags": [{"genre": "Fiction", "shelf": 4}]})),
    );
    assert_eq!(
        store.fit(&nested()).expect("checked"),
        Fit::Misfits(vec![retyped("tags", 1)]),
        "the closed struct does not read the stored tag"
    );
    assert_eq!(store.recorded().expect("the record reads"), Some(recorded));
}
