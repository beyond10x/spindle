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

// Round 2: the line the fix keeps whole, an optional field a row lacks, and invariant misfits
// beside field and state misfits on SQLite and PostgreSQL.

/// [`BEFORE`] guarded by `title != 'a<text>b'`, with `text` a YAML escape spliced into a
/// double-quoted statement; ESS's refusal where the specification does not compile.
fn broken_by_every_book(text: &str) -> Result<spindle_spec::EssIr, String> {
    let domain = BEFORE.replace(
        LAST_FIELD,
        &format!("{LAST_FIELD}    invariants:\n      - \"title != 'a{text}b'\"\n"),
    );
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/shelf.yaml".to_owned()],
        &[SYSTEM.to_owned(), domain],
    )
    .map_err(|error| format!("{text}: {error}"))
}

/// The one misfit line of a store holding a book titled by each of `titles` under
/// [`broken_by_every_book`]`(text)`, exactly one of which breaks it, with the statement the
/// misfit names; ESS's refusal where it does not compile it.
fn one_misfit(text: &str, titles: &[&str]) -> Result<(String, String), String> {
    let model = broken_by_every_book(text)?;
    let scratch = scratch();
    let recorded = Shapes::of(&ir(BEFORE));
    let rows: Vec<Value> = titles.iter().map(|title| json!({"title": title})).collect();
    let mut store = holding(&scratch.path().join("state.sqlite"), &recorded, &rows);
    let mut misfits = Shapes::of(&model)
        .misfits(&recorded, &mut store)
        .expect("the store is read");
    assert_eq!(misfits.len(), 1, "{text}: {misfits:?}");
    let misfit = misfits.remove(0);
    let Misfit::Invariant { invariant, .. } = &misfit else {
        panic!("{text}: an invariant misfit: {misfit:?}");
    };
    Ok((invariant.clone(), misfit.to_string()))
}

/// The fix escapes `\n` and `\r` only. A vertical tab, a form feed, NEL, LINE SEPARATOR and
/// PARAGRAPH SEPARATOR are line breaks too (Unicode's mandatory breaks), and a statement holding
/// one, which ESS compiles, is written raw into the misfit's line.
#[test]
fn adv2_every_unicode_line_break_in_an_invariant_keeps_the_misfit_on_one_line() {
    let mut raw = Vec::new();
    let mut judged = 0;
    let mut refused = Vec::new();
    for (escape, written) in [
        ("\\v", '\u{b}'),
        ("\\f", '\u{c}'),
        ("\\N", '\u{85}'),
        ("\\L", '\u{2028}'),
        ("\\P", '\u{2029}'),
    ] {
        let (statement, line) = match one_misfit(escape, &[&format!("a{written}b")]) {
            Ok(misfit) => misfit,
            Err(error) => {
                refused.push(error);
                continue;
            }
        };
        if !statement.contains(written) {
            continue;
        }
        judged += 1;
        if line.contains(written) {
            raw.push(format!("{escape}: {line:?}"));
        }
    }
    assert!(
        judged > 0,
        "ESS compiles none of the statements: {refused:#?}"
    );
    assert!(
        raw.is_empty(),
        "a line break other than \\n and \\r is written raw into the misfit's line: {raw:#?}"
    );
}

/// A statement holding a terminal control character (ESC, BEL, backspace, NUL, the C1 CSI),
/// which ESS compiles, is written raw into the line `run` prints on stdout, where ESC starts a
/// sequence that moves the cursor or clears the line. A tab is left alone.
#[test]
fn adv2_no_control_character_of_an_invariant_is_written_raw() {
    let mut raw = Vec::new();
    let mut judged = 0;
    let mut refused = Vec::new();
    for (escape, written) in [
        ("\\e", '\u{1b}'),
        ("\\a", '\u{7}'),
        ("\\b", '\u{8}'),
        ("\\0", '\u{0}'),
        ("\\x9b", '\u{9b}'),
    ] {
        let (statement, line) = match one_misfit(escape, &[&format!("a{written}b")]) {
            Ok(misfit) => misfit,
            Err(error) => {
                refused.push(error);
                continue;
            }
        };
        if !statement.contains(written) {
            continue;
        }
        judged += 1;
        if line.chars().any(|c| c.is_control() && c != '\t') {
            raw.push(format!("{escape}: {line:?}"));
        }
    }
    assert!(
        judged > 0,
        "ESS compiles none of the statements: {refused:#?}"
    );
    assert!(
        raw.is_empty(),
        "a control character of the statement is written raw: {raw:#?}"
    );
}

/// The escape is not reversible: a statement holding a line break and one holding a backslash
/// followed by `n` print the same misfit line, so the line does not say which statement broke.
#[test]
fn adv2_a_line_break_and_a_written_backslash_n_misfit_apart() {
    let (broken, broken_line) =
        one_misfit("\\n", &["a\nb", "a\\nb"]).expect("a line break compiles");
    assert!(broken.contains('\n'), "{broken:?}");
    let (written, written_line) =
        one_misfit("\\\\n", &["a\nb", "a\\nb"]).expect("a backslash compiles");
    assert!(
        written.contains("\\n") && !written.contains('\n'),
        "{written:?}"
    );
    assert_ne!(broken, written, "two statements");
    assert_ne!(
        broken_line, written_line,
        "two statements, one misfit line: {broken:?} and {written:?}"
    );
}

/// Kept: an invariant over an optional field leaves a row lacking the field and a row holding
/// `null` unknown, so both fit, as `at_rest` judges them; a row holding a value is judged.
#[test]
fn adv2_an_invariant_over_an_optional_field_a_row_lacks_fits() {
    let scratch = scratch();
    let recorded = Shapes::of(&ir(BEFORE));
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &recorded,
        &[json!({}), json!({"note": null}), json!({"note": "abcdef"})],
    );
    let model = guarded("      - 'note.count <= 4'\n      - \"note != 'abc'\"\n");
    let refused: Vec<String> = store
        .scan(BOOK)
        .expect("the books are read")
        .into_iter()
        .filter(|(identity, stored)| {
            spindle_kernel::at_rest(&model, BOOK, identity, &stored.row).is_err()
        })
        .map(|(identity, _)| identity.to_string())
        .collect();
    assert_eq!(refused, vec!["\"b2\"".to_owned()]);
    assert_eq!(
        Shapes::of(&model)
            .misfits(&recorded, &mut store)
            .expect("the store is read"),
        vec![Misfit::Invariant {
            entity: BOOK.to_owned(),
            invariant: "note.count <= 4".to_owned(),
            rows: 1,
        }]
    );
}

const LENDING: &str = "domain: keep.shelf
summary: Books on shelves, some lent.
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: String}
    fields:
      - {name: title, type: String}
      - {name: weight, type: Integer}
      - {name: note, type: Optional<String>}
    lifecycle:
      initial: Shelved
      states: [Shelved, Lent]
      terminal: [Lent]
      transitions:
        - {name: lend, from: [Shelved], to: Lent}
commands:
  - name: keep.shelf.Lend
    input:
      - {name: id, type: String}
    outcomes:
      - name: lent
        moves: keep.shelf.Book.lend
        instance: id
        emits: [keep.shelf.BookLent]
        payload:
          keep.shelf.BookLent: {id: input.id}
        summary: The book is lent.
events:
  - name: keep.shelf.BookLent
    fields:
      - {name: id, type: String}
";

/// [`LENDING`] with `weight` removed, a required `isbn` added, the state `Lent` removed and four
/// invariants, one of them over `isbn` and one whose statement holds a line break.
const LENT_GONE: &str = "domain: keep.shelf
summary: Books on shelves.
entities:
  - name: keep.shelf.Book
    identity: {name: id, type: String}
    fields:
      - {name: title, type: String}
      - {name: note, type: Optional<String>}
      - {name: isbn, type: String}
    invariants:
      - 'title.count <= 10'
      - 'isbn.count == 13'
      - 'note.count <= 4'
      - \"title != 'one\\ntwo'\"
    lifecycle: {initial: Shelved, states: [Shelved], terminal: [Shelved]}
";

/// Commits one book with `identity`, resting in `state`, holding `fields`.
fn put(store: &mut dyn Store, identity: &str, state: &str, fields: &Value) {
    store
        .commit(&Change {
            entity: BOOK.to_owned(),
            identity: json!(identity),
            expected: None,
            row: Row {
                state: state.to_owned(),
                fields: fields
                    .as_object()
                    .expect("an object")
                    .clone()
                    .into_iter()
                    .collect(),
            },
            events: vec![spindle_kernel::Event {
                name: "keep.shelf.Put".to_owned(),
                payload: serde_json::Map::from_iter([("id".to_owned(), json!(identity))]),
            }],
            scanned: Vec::new(),
        })
        .expect("the row is committed");
}

/// Three books under [`LENDING`]: one whose long title breaks two invariants, one lent with a
/// `null` note, one titled with a line break whose note breaks `note.count <= 4`.
fn lending(store: &mut dyn Store) {
    put(
        store,
        "b0",
        "Shelved",
        &json!({"title": "The Left Hand of Darkness", "weight": 1}),
    );
    put(
        store,
        "b1",
        "Lent",
        &json!({"title": "Dune", "weight": 2, "note": null}),
    );
    put(
        store,
        "b2",
        "Shelved",
        &json!({"title": "one\ntwo", "weight": 3, "note": "abcdef"}),
    );
}

/// Each misfit line of [`lending`]'s books under [`LENT_GONE`], sorted: three invariant lines,
/// the removed field, the required field and the removed state; `isbn.count == 13`, left unknown
/// by every book, gives none.
fn lent_gone_lines() -> Vec<String> {
    let mut lines = vec![
        "keep.shelf.Book invariant `title.count <= 10`: the invariant does not hold; stored rows \
         breaking it: 1"
            .to_owned(),
        "keep.shelf.Book invariant `note.count <= 4`: the invariant does not hold; stored rows \
         breaking it: 1"
            .to_owned(),
        "keep.shelf.Book invariant `title != 'one\\ntwo'`: the invariant does not hold; stored rows \
         breaking it: 1"
            .to_owned(),
        "keep.shelf.Book.weight: the field is removed; stored rows holding it: 3".to_owned(),
        "keep.shelf.Book.isbn: the field is required; stored rows lacking it: 3".to_owned(),
        "keep.shelf.Book state Lent: the state is removed; stored rows resting in it: 1"
            .to_owned(),
    ];
    lines.sort();
    lines
}

fn sorted_lines(misfits: &[Misfit]) -> Vec<String> {
    let mut lines: Vec<String> = misfits.iter().map(ToString::to_string).collect();
    lines.sort();
    lines
}

/// Kept: invariant misfits stand beside the field and state misfits of the same entity, each its
/// own line; a required field the rows lack leaves its invariant unknown, so it adds no line.
#[test]
fn adv2_invariant_field_and_state_misfits_of_one_entity_each_take_a_line() {
    let scratch = scratch();
    let recorded = Shapes::of(&ir(LENDING));
    let mut store = SqliteStore::open(&scratch.path().join("state.sqlite")).expect("opens");
    store.record(&recorded).expect("the record is written");
    lending(&mut store);
    let misfits = Shapes::of(&ir(LENT_GONE))
        .misfits(&recorded, &mut store)
        .expect("the store is read");
    assert_eq!(sorted_lines(&misfits), lent_gone_lines(), "{misfits:#?}");
}

/// Kept, on PostgreSQL: the same books give the same misfit lines as on SQLite, and `fit`
/// records nothing.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_invariant_field_and_state_misfits_match_sqlite_on_postgres() {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let schema = format!(
        "spindle_advw33u4_{}",
        uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .get(..12)
            .expect("hex")
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
                .await
                .expect("the test database accepts its owner");
            let driver = tokio::spawn(connection);
            client
                .simple_query(&format!("CREATE SCHEMA {schema}"))
                .await
                .expect("the schema is created");
            drop(client);
            let _ = driver.await;
        });
    let mut database = spindle_kernel::Database::new(url);
    database.schema = schema;
    let recorded = Shapes::of(&ir(LENDING));
    let mut store =
        spindle_kernel::PostgresStore::open(&database, "keep", 1).expect("the tables open");
    store.record(&recorded).expect("the record is written");
    lending(&mut store);
    let after = Shapes::of(&ir(LENT_GONE));
    let Fit::Misfits(misfits) = store.fit(&after).expect("checked") else {
        panic!("the books misfit");
    };
    assert_eq!(sorted_lines(&misfits), lent_gone_lines(), "{misfits:#?}");
    assert_eq!(
        store.recorded().expect("the record reads"),
        Some(recorded),
        "the record is unchanged"
    );
}

// Round 3: the escape every misfit line is written through, as a property over every variant and
// every text, and an invariant that reads what a row at rest binds beside its fields.

/// The escape the documentation of `OneLine` (`fit.rs`) states, written here apart from it: a
/// backslash `\\`, a line break `\n`, a carriage return `\r`, every other control character but a
/// tab and U+2028 and U+2029 `\u{..}` in lower-case hexadecimal, everything else as it is.
fn escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push('\t'),
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{{{:x}}}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out
}

/// The inverse of [`escaped`]: `None` where `line` is no escape's output, so a text read back
/// equal to the one written proves the escape loses nothing.
fn unescaped(line: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let mut hex = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        digit => hex.push(digit),
                    }
                }
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Every text of every variant, each built with `text` there and `x` in the variant's other
/// texts: the variant's and the text's name, and the misfit.
fn every_text(text: &str) -> Vec<(&'static str, Misfit)> {
    let x = || "x".to_owned();
    let t = || text.to_owned();
    vec![
        (
            "Entity.entity",
            Misfit::Entity {
                entity: t(),
                rows: 1,
            },
        ),
        (
            "Moved.entity",
            Misfit::Moved {
                entity: t(),
                component: x(),
                rows: 1,
            },
        ),
        (
            "Moved.component",
            Misfit::Moved {
                entity: x(),
                component: t(),
                rows: 1,
            },
        ),
        (
            "Removed.entity",
            Misfit::Removed {
                entity: t(),
                field: x(),
                rows: 1,
            },
        ),
        (
            "Removed.field",
            Misfit::Removed {
                entity: x(),
                field: t(),
                rows: 1,
            },
        ),
        (
            "Retyped.entity",
            Misfit::Retyped {
                entity: t(),
                field: x(),
                rows: 1,
            },
        ),
        (
            "Retyped.field",
            Misfit::Retyped {
                entity: x(),
                field: t(),
                rows: 1,
            },
        ),
        (
            "Required.entity",
            Misfit::Required {
                entity: t(),
                field: x(),
                rows: 1,
            },
        ),
        (
            "Required.field",
            Misfit::Required {
                entity: x(),
                field: t(),
                rows: 1,
            },
        ),
        (
            "State.entity",
            Misfit::State {
                entity: t(),
                state: x(),
                rows: 1,
            },
        ),
        (
            "State.state",
            Misfit::State {
                entity: x(),
                state: t(),
                rows: 1,
            },
        ),
        (
            "Invariant.entity",
            Misfit::Invariant {
                entity: t(),
                invariant: x(),
                rows: 1,
            },
        ),
        (
            "Invariant.invariant",
            Misfit::Invariant {
                entity: x(),
                invariant: t(),
                rows: 1,
            },
        ),
    ]
}

/// Kept, a property (fixed seed): for drawn texts over backslashes, `u`, braces, hexadecimal
/// digits, `n`, `r`, every line break, controls, a tab, a backtick and characters of 2, 3 and 4
/// bytes, each text of each of the 7 variants prints as the variant prints a placeholder with
/// the placeholder replaced by [`escaped`], [`unescaped`] reads the text back, the line holds no
/// control character but a tab, and no two drawn texts print one line.
#[test]
fn adv3_every_text_of_every_misfit_is_written_by_one_reversible_escape() {
    const PLACEHOLDER: &str = "\u{e000}";
    let alphabet = [
        '\\',
        'u',
        '{',
        '}',
        '1',
        'b',
        'n',
        'r',
        '0',
        'a',
        '`',
        ' ',
        '\n',
        '\r',
        '\t',
        '\u{0}',
        '\u{7}',
        '\u{8}',
        '\u{b}',
        '\u{c}',
        '\u{1b}',
        '\u{7f}',
        '\u{85}',
        '\u{9b}',
        '\u{2028}',
        '\u{2029}',
        'é',
        '€',
        '\u{1d11e}',
        '\u{feff}',
        '\u{202e}',
    ];
    let mut seed: u64 = 0x5EED_0033_0403;
    let mut draw = |below: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(seed >> 33).expect("31 bits") % below
    };
    let mut texts: Vec<String> = vec![String::new(), "\\u{1b}".to_owned(), "\u{1b}".to_owned()];
    for _ in 0..3000 {
        let length = draw(9);
        texts.push(
            (0..length)
                .map(|_| alphabet[draw(alphabet.len())])
                .collect(),
        );
    }
    texts.sort();
    texts.dedup();
    let templates: Vec<(&str, String)> = every_text(PLACEHOLDER)
        .into_iter()
        .map(|(name, misfit)| (name, misfit.to_string()))
        .collect();
    for (name, template) in &templates {
        assert_eq!(
            template.matches(PLACEHOLDER).count(),
            1,
            "{name}: {template:?}"
        );
    }
    let mut seen: Vec<BTreeMap<String, String>> = vec![BTreeMap::new(); templates.len()];
    for text in &texts {
        let written = escaped(text);
        assert_eq!(
            unescaped(&written).as_deref(),
            Some(text.as_str()),
            "{text:?}"
        );
        for (index, (name, misfit)) in every_text(text).into_iter().enumerate() {
            let line = misfit.to_string();
            assert_eq!(
                line,
                templates[index].1.replacen(PLACEHOLDER, &written, 1),
                "{name} of {text:?}"
            );
            assert!(
                line.chars()
                    .all(|c| (!c.is_control() || c == '\t') && c != '\u{2028}' && c != '\u{2029}'),
                "{name}: {line:?}"
            );
            if let Some(other) = seen[index].insert(line.clone(), text.clone()) {
                panic!("{name}: {other:?} and {text:?} print one line: {line:?}");
            }
        }
    }
}

/// Kept, exhaustive: every scalar value, written between two letters into an entity's name,
/// prints as [`escaped`] writes it and reads back.
#[test]
fn adv3_every_character_is_written_by_the_escape_and_reads_back() {
    const TAIL: &str = ": the entity is removed; stored rows of it: 1";
    for c in (0..=0x10_FFFF_u32).filter_map(char::from_u32) {
        let text = format!("a{c}b");
        let line = Misfit::Entity {
            entity: text.clone(),
            rows: 1,
        }
        .to_string();
        let Some(written) = line.strip_suffix(TAIL) else {
            panic!("{c:?}: {line:?}");
        };
        assert_eq!(written, escaped(&text), "{c:?}");
        assert_eq!(unescaped(written).as_deref(), Some(text.as_str()), "{c:?}");
    }
}

/// The books `b0`, `b1` and `b2`, resting in `Shelved`, under `model`: the identities `at_rest`
/// refuses, and the store's misfits.
fn judged_by_at_rest(model: &spindle_spec::EssIr) -> (Vec<Value>, Vec<Misfit>) {
    let scratch = scratch();
    let recorded = Shapes::of(&ir(BEFORE));
    let mut store = holding(
        &scratch.path().join("state.sqlite"),
        &recorded,
        &[json!({}), json!({}), json!({})],
    );
    let refused = store
        .scan(BOOK)
        .expect("the books are read")
        .into_iter()
        .filter(|(identity, stored)| {
            spindle_kernel::at_rest(model, BOOK, identity, &stored.row).is_err()
        })
        .map(|(identity, _)| identity)
        .collect();
    let misfits = Shapes::of(model)
        .misfits(&recorded, &mut store)
        .expect("the store is read");
    (refused, misfits)
}

/// Kept: an invariant over the identity is judged with the stored row's identity bound, as
/// `at_rest` binds it (`Facts::row`): `id != 'b1'` breaks one of three books. The unit's cases and
/// rounds 1 and 2 read only fields, so facts built without the identity (`&Value::Null` in place
/// of `&identity` in `Held::of`) stay green there.
#[test]
fn adv3_an_invariant_over_the_identity_is_judged_as_at_rest_judges_it() {
    let (refused, misfits) = judged_by_at_rest(&guarded("      - \"id != 'b1'\"\n"));
    assert_eq!(refused, vec![json!("b1")], "at_rest refuses b1");
    assert_eq!(
        misfits,
        vec![Misfit::Invariant {
            entity: BOOK.to_owned(),
            invariant: "id != 'b1'".to_owned(),
            rows: 1,
        }]
    );
}

/// Kept: an invariant over the state is judged with the stored row's state bound, as `at_rest`
/// binds it: `state != 'Shelved'` breaks all three books.
#[test]
fn adv3_an_invariant_over_the_state_is_judged_as_at_rest_judges_it() {
    let (refused, misfits) = judged_by_at_rest(&guarded("      - \"state != 'Shelved'\"\n"));
    assert_eq!(refused.len(), 3, "at_rest refuses every book: {refused:?}");
    assert_eq!(
        misfits,
        vec![Misfit::Invariant {
            entity: BOOK.to_owned(),
            invariant: "state != 'Shelved'".to_owned(),
            rows: 3,
        }]
    );
}
