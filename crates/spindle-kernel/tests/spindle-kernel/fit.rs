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
  - name: keep.shelf.Code
    kind: newtype
    of: String
  - name: keep.shelf.Place
    kind: struct
    fields:
      - {name: room, type: String}
      - {name: floor, type: Integer}
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: keep.shelf.BookId}
    fields:
      - {name: title, type: String}
      - {name: weight, type: keep.shelf.Weight}
      - {name: note, type: Optional<String>}
      - {name: code, type: Optional<keep.shelf.Code>}
      - {name: place, type: Optional<keep.shelf.Place>}
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
        model: spindle_kernel::Model::default(),
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

/// A store at `path` that recorded [`BEFORE`] and holds a book of each of `fields`, `b1` on.
fn holding(path: &Path, fields: &[Value]) -> SqliteStore {
    let mut store = open(path);
    assert_eq!(
        store.fit(&before()).expect("the store is checked"),
        Fit::Fits { unrecorded: false }
    );
    for (index, fields) in fields.iter().enumerate() {
        let mut row = json!({"title": "Dune", "weight": 3});
        row.as_object_mut()
            .expect("an object")
            .extend(fields.as_object().expect("an object").clone());
        put(&mut store, BOOK, &format!("b{}", index + 1), "Shelved", row);
    }
    store
}

/// `after` fits the rows of `store`, and is recorded.
fn fits(store: &mut SqliteStore, after: &Shapes) {
    assert_eq!(
        store.fit(after).expect("checked"),
        Fit::Fits { unrecorded: false },
        "every stored value reads under the new specification"
    );
    assert_eq!(
        store.recorded().expect("the record reads").as_ref(),
        Some(after)
    );
}

fn retyped(field: &str, rows: usize) -> Vec<Misfit> {
    vec![Misfit::Retyped {
        entity: BOOK.to_owned(),
        field: field.to_owned(),
        rows,
    }]
}

const PLACE: &str = "      - {name: floor, type: Integer}\n";
const CODE: &str = "  - name: keep.shelf.Code\n    kind: newtype\n    of: String\n";

/// F2: a struct that gains an optional member reads every stored value of it.
#[test]
fn a_struct_that_gains_an_optional_member_fits_stored_rows() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"place": {"room": "A", "floor": 1}})],
    );
    fits(
        &mut store,
        &changed(
            PLACE,
            &format!("{PLACE}      - {{name: bay, type: Optional<String>}}\n"),
        ),
    );
}

/// F2, the other side: a struct that gains a required member reads no stored value of it.
#[test]
fn a_struct_that_gains_a_required_member_misfits_rows_holding_it() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"place": {"room": "A", "floor": 1}}), json!({})],
    );
    let after = changed(
        PLACE,
        &format!("{PLACE}      - {{name: bay, type: String}}\n"),
    );
    assert_eq!(misfits(&mut store, &after), retyped("place", 1));
}

/// F5: a newtype that gains a prefix every stored value starts with reads them all.
#[test]
fn a_newtype_that_gains_a_prefix_every_stored_value_has_fits() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"code": "bk-1"}), json!({"code": "bk-22"})],
    );
    fits(
        &mut store,
        &changed(CODE, &format!("{CODE}    prefix: \"bk-\"\n")),
    );
}

/// F5: a newtype that gains a prefix a stored value lacks, or an alphabet or an invariant a
/// stored value breaks, misfits the rows holding the field.
#[test]
fn a_newtype_constraint_a_stored_value_breaks_misfits() {
    let constraints = [
        "    prefix: \"bk-\"\n",
        "    alphabet: \"bk-0123456789\"\n",
        "    invariants:\n      - 'value.count <= 4'\n",
    ];
    for constraint in constraints {
        let scratch = scratch();
        let mut store = holding(
            &scratch.path().join("state.sqlite"),
            &[json!({"code": "bk-1"}), json!({"code": "xy-12"}), json!({})],
        );
        let after = changed(CODE, &format!("{CODE}{constraint}"));
        assert_eq!(
            misfits(&mut store, &after),
            retyped("code", 2),
            "{constraint}"
        );
    }
}

/// F5: a struct that gains an invariant a stored value breaks misfits the rows holding it.
#[test]
fn a_struct_invariant_a_stored_value_breaks_misfits() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"place": {"room": "A", "floor": -1}})],
    );
    let after = changed(
        PLACE,
        &format!("{PLACE}    invariants:\n      - 'floor >= 0'\n"),
    );
    assert_eq!(misfits(&mut store, &after), retyped("place", 1));
}

/// A closed struct opened to undeclared members reads every stored value of it: the change is
/// part of the structure, so the values are read again, and each still reads.
#[test]
fn a_struct_opened_to_undeclared_members_fits_stored_rows() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"place": {"room": "A", "floor": 1}}), json!({})],
    );
    let after = changed(
        "    kind: struct\n",
        "    kind: struct\n    undeclared_fields: ignored\n",
    );
    assert_ne!(
        after.entities[BOOK].fields["place"],
        before().entities[BOOK].fields["place"],
        "opening the struct changes its structure"
    );
    fits(&mut store, &after);
}

const TWO: &str = "format: ess/34
system: keep
version: v1
summary: Books and safes kept in two durable stores.
domains:
  - keep.shelf
  - keep.vault
components:
  - component: shelf
    summary: Keeps one domain.
    owns:
      domains:
        - keep.shelf
  - component: archive
    summary: Keeps the other.
    owns:
      domains:
        - keep.vault
";

const VAULT: &str = "domain: keep.vault
summary: Safes.
entities:
  - name: keep.vault.Safe
    identity: {name: id, type: String}
    fields:
      - {name: label, type: String}
    lifecycle: {initial: Locked, states: [Locked], terminal: [Locked]}
";

/// The model of [`BEFORE`] and [`VAULT`] under `system`.
fn two(system: &str) -> spindle_spec::EssIr {
    spindle_spec::compile(
        &[
            "system.yaml".to_owned(),
            "domains/shelf.yaml".to_owned(),
            "domains/vault.yaml".to_owned(),
        ],
        &[system.to_owned(), BEFORE.to_owned(), VAULT.to_owned()],
    )
    .unwrap_or_else(|error| panic!("the specification compiles: {error}\n{system}"))
}

/// F4: the component `shelf` stored books; its domain is now owned by `archive`. The books are
/// not removed from the model: the misfit names the component they moved to.
#[test]
fn an_entity_whose_domain_moved_to_another_component_misfits_naming_it() {
    let scratch = scratch();
    let mut store = open(&scratch.path().join("state.sqlite"));
    let before = Shapes::owned(&two(TWO), "shelf");
    assert_eq!(
        store.fit(&before).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    put(
        &mut store,
        BOOK,
        "b1",
        "Shelved",
        json!({"title": "Dune", "weight": 3}),
    );
    let swapped = TWO
        .replace("        - keep.shelf\n", "        - keep.SWAP\n")
        .replace("        - keep.vault\n", "        - keep.shelf\n")
        .replace("        - keep.SWAP\n", "        - keep.vault\n");
    let after = Shapes::owned(&two(&swapped), "shelf");
    let Fit::Misfits(misfits) = store.fit(&after).expect("checked") else {
        panic!("the move misfits")
    };
    assert_eq!(
        misfits,
        vec![Misfit::Moved {
            entity: BOOK.to_owned(),
            component: "archive".to_owned(),
            rows: 1,
        }]
    );
    let line = misfits[0].to_string();
    assert!(
        line.contains(BOOK) && line.contains("archive") && !line.contains("removed"),
        "the line names the move: {line}"
    );
    assert_eq!(store.recorded().expect("the record reads"), Some(before));
}

// `story:entity-invariant-over-stored-rows`: every invariant the new model declares on an entity
// is judged over every stored row of it, as a row at rest is judged; `Unknown` concludes nothing.

const TITLED: &str = "'title.count <= 10'";
const LIGHT: &str = "'weight <= 2'";
const NOTED: &str = "'note.count <= 4'";
const LAST_FIELD: &str = "      - {name: place, type: Optional<keep.shelf.Place>}\n";

/// The model of [`BEFORE`] with `statements` declared as invariants of the book, the domain
/// then edited by `edit`.
fn invariant_model(statements: &[&str], edit: impl Fn(String) -> String) -> spindle_spec::EssIr {
    let declared: String = statements
        .iter()
        .map(|statement| format!("      - {statement}\n"))
        .collect();
    let domain = BEFORE.replace(
        LAST_FIELD,
        &format!("{LAST_FIELD}    invariants:\n{declared}"),
    );
    assert_ne!(
        domain, BEFORE,
        "the book's last field is in the specification"
    );
    ir(&edit(domain))
}

fn guarded(statements: &[&str]) -> Shapes {
    Shapes::of(&invariant_model(statements, |domain| domain))
}

fn statement(quoted: &str) -> String {
    quoted.trim_matches('\'').to_owned()
}

fn broken(quoted: &str, rows: usize) -> Misfit {
    Misfit::Invariant {
        entity: BOOK.to_owned(),
        invariant: statement(quoted),
        rows,
    }
}

#[test]
fn an_entity_invariant_a_stored_row_breaks_misfits() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({}), json!({"title": "The Left Hand of Darkness"})],
    );
    let after = guarded(&[TITLED]);
    let misfits = after
        .misfits(&before(), &mut store)
        .expect("the store is read");
    assert_eq!(misfits, vec![broken(TITLED, 1)]);
    let line = misfits[0].to_string();
    assert!(
        line.contains(BOOK) && line.contains(&statement(TITLED)) && line.ends_with(": 1"),
        "the misfit names the entity, the statement and the row count: {line}"
    );
    assert_eq!(misfits[0].entity(), BOOK);
    assert_eq!(misfits[0].field(), None);
    assert_eq!(
        store.fit(&after).expect("checked"),
        Fit::Misfits(misfits),
        "the store refuses the specification"
    );
    assert_eq!(
        store.recorded().expect("the record reads"),
        Some(before()),
        "a misfit leaves the record as it was"
    );
}

#[test]
fn an_entity_invariant_every_stored_row_holds_fits() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({}), json!({"title": "Emma"})],
    );
    let after = guarded(&[TITLED]);
    assert_eq!(
        after
            .misfits(&before(), &mut store)
            .expect("the store is read"),
        Vec::new()
    );
    fits(&mut store, &after);
}

/// A book without a note leaves `note.count <= 4` `Unknown`, as a row at rest leaves it
/// ([`spindle_kernel::at_rest`]): it fits. A book whose note is too long breaks it.
#[test]
fn an_entity_invariant_a_stored_row_leaves_unknown_fits() {
    let scratch = scratch();
    let mut store = holding(&scratch.path().join("state.sqlite"), &[json!({})]);
    let model = invariant_model(&[NOTED], |domain| domain);
    let (identity, stored) = store
        .scan(BOOK)
        .expect("the books are read")
        .into_iter()
        .next()
        .expect("one book");
    assert!(
        spindle_kernel::at_rest(&model, BOOK, &identity, &stored.row).is_ok(),
        "a row at rest without a note breaks nothing"
    );
    let after = Shapes::of(&model);
    assert_eq!(
        after
            .misfits(&before(), &mut store)
            .expect("the store is read"),
        Vec::new()
    );
    fits(&mut store, &after);
    let scratch = self::scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({}), json!({"note": "a long note"})],
    );
    assert_eq!(
        after
            .misfits(&before(), &mut store)
            .expect("the store is read"),
        vec![broken(NOTED, 1)],
        "the same invariant over a note it reads concludes"
    );
}

/// D1 A: the record keeps no invariant, so one both specifications declare is judged again when
/// the digest differs, over a row written under a third specification that lacked it.
#[test]
fn an_invariant_both_models_declare_is_judged_over_the_stored_rows() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"title": "The Left Hand of Darkness"})],
    );
    let recorded = guarded(&[TITLED]);
    store.record(&recorded).expect("the record is written");
    let recorded = store
        .recorded()
        .expect("the record reads")
        .expect("a record");
    let after = Shapes::of(&invariant_model(&[TITLED], |domain| {
        domain.replace(
            "      - {name: note, type: Optional<String>}\n",
            "      - {name: note, type: Optional<String>}\n      - {name: shelfmark, type: Optional<String>}\n",
        )
    }));
    assert_ne!(after.digest, recorded.digest, "the digests differ");
    assert_eq!(
        after
            .misfits(&recorded, &mut store)
            .expect("the store is read"),
        vec![broken(TITLED, 1)]
    );
    let shape = serde_json::to_value(&recorded.entities[BOOK]).expect("the shape serialises");
    let keys: Vec<&str> = shape
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        vec!["fields", "states"],
        "the record format is unchanged"
    );
    assert_eq!(
        serde_json::to_value(&recorded.entities).expect("the shapes serialise"),
        serde_json::to_value(&before().entities).expect("the shapes serialise"),
        "an invariant leaves no trace in the record"
    );
}

/// D2 A: one misfit for each broken invariant, each one line.
#[test]
fn a_row_breaking_two_invariants_misfits_once_per_invariant() {
    let scratch = scratch();
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &[json!({"title": "The Left Hand of Darkness", "weight": 3})],
    );
    let after = guarded(&[TITLED, LIGHT]);
    let misfits = after
        .misfits(&before(), &mut store)
        .expect("the store is read");
    assert_eq!(misfits, vec![broken(TITLED, 1), broken(LIGHT, 1)]);
    let text: String = misfits.iter().map(|misfit| format!("{misfit}\n")).collect();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    for (line, quoted) in lines.iter().zip([TITLED, LIGHT]) {
        assert!(
            line.contains(BOOK) && line.contains(&statement(quoted)) && line.ends_with(": 1"),
            "{line}"
        );
    }
}
