//! Adversary, wave 30 U10 (`story:related-via-nested-input`): every other way of naming a related
//! row through a member of a struct input, or of a struct the subject stores, is refused when the
//! specification loads, so the kernel's flat read of the reference (`Run::reference`) never meets
//! one. Each case first loads the same specification with the row named through a top-level field,
//! so the refusal is the nested name's and not the shape's.

use std::fs;
use std::path::Path;

/// One specification of `demo.shelf`: `inputs` and `outcomes` are spliced into the command
/// `demo.shelf.PlaceBook`, whose last outcome creates the book and emits `payload` with
/// `event_fields`. `book_fields` are the stored fields of the book.
struct Shape<'a> {
    format: &'a str,
    inputs: &'a str,
    outcomes: &'a str,
    payload: &'a str,
    event_fields: &'a str,
    book_fields: &'a str,
    extra_commands: &'a str,
    sets: &'a str,
}

impl Default for Shape<'_> {
    fn default() -> Self {
        Self {
            format: "ess/34",
            inputs: "",
            outcomes: "",
            payload: "{book_id: input.book_id}",
            event_fields: "[{name: book_id, type: demo.shelf.BookId}]",
            book_fields: "[]",
            extra_commands: "",
            sets: "",
        }
    }
}

fn write(root: &Path, shape: &Shape<'_>) {
    fs::write(
        root.join("system.yaml"),
        format!(
            "format: {}\nsystem: demo\nversion: v1\ndomains:\n  - demo.shelf\n",
            shape.format
        ),
    )
    .expect("write");
    fs::create_dir_all(root.join("domains")).expect("the domains directory");
    let Shape {
        inputs,
        outcomes,
        payload,
        event_fields,
        book_fields,
        extra_commands,
        sets,
        ..
    } = shape;
    fs::write(
        root.join("domains/shelf.yaml"),
        format!(
            "domain: demo.shelf
types:
  - {{name: demo.shelf.ShelfId, kind: newtype, of: String}}
  - {{name: demo.shelf.BookId, kind: newtype, of: String}}
  - name: demo.shelf.Wrap
    kind: struct
    fields:
      - {{name: shelf, type: demo.shelf.ShelfId}}
entities:
  - name: demo.shelf.Shelf
    identity: {{name: shelf_id, type: demo.shelf.ShelfId}}
    fields:
      - {{name: capacity, type: Integer}}
    lifecycle: {{initial: Open, states: [Open], terminal: [Open]}}
  - name: demo.shelf.Book
    identity: {{name: book_id, type: demo.shelf.BookId}}
    fields: {book_fields}
    lifecycle: {{initial: Placed, states: [Placed], terminal: [Placed]}}
errors:
  - {{name: demo.shelf.NoShelf, summary: No shelf carries the id., fields: []}}
  - {{name: demo.shelf.Full, summary: The shelf is full., fields: []}}
commands:
  - name: demo.shelf.PlaceBook
    input:
      - {{name: book_id, type: demo.shelf.BookId}}
{inputs}
    outcomes:
{outcomes}
      - name: placed
        creates: demo.shelf.Book
        instance: book_id
{sets}
        emits: [demo.shelf.BookPlaced]
        payload:
          demo.shelf.BookPlaced: {payload}
        summary: The book is placed.
{extra_commands}
events:
  - name: demo.shelf.BookPlaced
    fields: {event_fields}
  - name: demo.shelf.BookMoved
    fields: [{{name: book_id, type: demo.shelf.BookId}}]
"
        ),
    )
    .expect("write");
}

/// Loads `top` (the row named through a top-level field) and then `nested` (through a member),
/// each written into its own temporary directory; returns the refusal of `nested`.
fn control_then_refusal(top: &Shape<'_>, nested: &Shape<'_>) -> String {
    let control = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    write(control.path(), top);
    if let Err(error) = spindle_spec::load(control.path()) {
        panic!("the control through a top-level field loads:\n{error}");
    }
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    write(directory.path(), nested);
    match spindle_spec::load(directory.path()) {
        Ok(_) => panic!(
            "ESS accepted a related row named through a nested member; the kernel's read of \
             it (`Run::reference`, flat) must now be compared with `ess_conformance::interpret`"
        ),
        Err(error @ spindle_spec::LoadError::Refused(_)) => error.to_string(),
        Err(other) => panic!("refused for another reason: {other}"),
    }
}

/// The refusal names `name` on a line of `code`.
fn names(text: &str, code: &str, name: &str) {
    assert!(
        text.lines()
            .any(|line| line.contains(code) && line.contains(name)),
        "no `{code}` line names {name}:\n{text}"
    );
}

const TOP_AND_WRAP: &str = "      - {name: other, type: demo.shelf.ShelfId}
      - {name: wrap, type: demo.shelf.Wrap}";

fn missing(via: &str) -> String {
    format!(
        "      - name: no-shelf
        when_related: {{via: {via}, exists: false}}
        error: demo.shelf.NoShelf
        summary: The named shelf does not exist."
    )
}

#[test]
fn an_optional_struct_member_is_refused() {
    let inputs = "      - {name: other, type: Optional<demo.shelf.ShelfId>}
      - {name: wrap, type: Optional<demo.shelf.Wrap>}";
    let top = missing("input.other");
    let nested = missing("input.wrap.shelf");
    let text = control_then_refusal(
        &Shape {
            inputs,
            outcomes: &top,
            ..Shape::default()
        },
        &Shape {
            inputs,
            outcomes: &nested,
            ..Shape::default()
        },
    );
    names(&text, "[undeclared_reference]", "`wrap.shelf`");
}

#[test]
fn a_nested_member_at_the_first_format_of_input_paths_is_refused() {
    let top = missing("input.other");
    let nested = missing("input.wrap.shelf");
    let text = control_then_refusal(
        &Shape {
            format: "ess/22",
            inputs: TOP_AND_WRAP,
            outcomes: &top,
            ..Shape::default()
        },
        &Shape {
            format: "ess/22",
            inputs: TOP_AND_WRAP,
            outcomes: &nested,
            ..Shape::default()
        },
    );
    names(&text, "[undeclared_reference]", "`wrap.shelf`");
}

#[test]
fn a_nested_member_beside_a_top_level_row_is_refused_in_either_order() {
    let inputs = "      - {name: shelf, type: demo.shelf.ShelfId}
      - {name: other, type: demo.shelf.ShelfId}
      - {name: wrap, type: demo.shelf.Wrap}";
    let second = |via: &str| {
        format!(
            "      - name: no-second
        when_related: {{via: {via}, exists: false}}
        error: demo.shelf.NoShelf
        summary: The second shelf does not exist."
        )
    };
    let top = format!("{}\n{}", missing("input.shelf"), second("input.other"));
    for nested in [
        format!("{}\n{}", missing("input.shelf"), second("input.wrap.shelf")),
        format!("{}\n{}", missing("input.wrap.shelf"), second("input.shelf")),
    ] {
        let text = control_then_refusal(
            &Shape {
                inputs,
                outcomes: &top,
                ..Shape::default()
            },
            &Shape {
                inputs,
                outcomes: &nested,
                ..Shape::default()
            },
        );
        names(&text, "[undeclared_reference]", "`wrap.shelf`");
    }
}

#[test]
fn a_nested_member_in_a_predicate_guard_is_refused() {
    let full = |via: &str| {
        format!(
            "{}
      - name: full
        when_related: {{via: {via}, predicate: capacity == 0}}
        error: demo.shelf.Full
        summary: The shelf is full.",
            missing(via)
        )
    };
    let top = full("input.other");
    let nested = full("input.wrap.shelf");
    let text = control_then_refusal(
        &Shape {
            inputs: TOP_AND_WRAP,
            outcomes: &top,
            ..Shape::default()
        },
        &Shape {
            inputs: TOP_AND_WRAP,
            outcomes: &nested,
            ..Shape::default()
        },
    );
    names(&text, "[undeclared_reference]", "`wrap.shelf`");
}

#[test]
fn a_nested_member_in_an_all_branch_is_refused() {
    let inputs = "      - {name: shelf, type: demo.shelf.ShelfId}
      - {name: other, type: demo.shelf.ShelfId}
      - {name: wrap, type: demo.shelf.Wrap}";
    let both = |via: &str| {
        format!(
            "{}
      - name: no-second
        when_related: {{via: {via}, exists: false}}
        error: demo.shelf.NoShelf
        summary: The second shelf does not exist.
      - name: full
        when_related:
          all:
            - {{via: input.shelf, predicate: capacity == 0}}
            - {{via: {via}, predicate: capacity == 0}}
        error: demo.shelf.Full
        summary: Both shelves are full.",
            missing("input.shelf")
        )
    };
    let top = both("input.other");
    let nested = both("input.wrap.shelf");
    let text = control_then_refusal(
        &Shape {
            inputs,
            outcomes: &top,
            ..Shape::default()
        },
        &Shape {
            inputs,
            outcomes: &nested,
            ..Shape::default()
        },
    );
    names(&text, "[undeclared_reference]", "`wrap.shelf`");
}

#[test]
fn a_member_of_a_list_of_structs_is_refused() {
    let inputs = "      - {name: others, type: List<demo.shelf.ShelfId>}
      - {name: wraps, type: List<demo.shelf.Wrap>}";
    let top = missing("input.others");
    let nested = missing("input.wraps.shelf");
    let text = control_then_refusal(
        &Shape {
            inputs,
            outcomes: &top,
            ..Shape::default()
        },
        &Shape {
            inputs,
            outcomes: &nested,
            ..Shape::default()
        },
    );
    names(&text, "[undeclared_reference]", "`wraps.shelf`");
}

#[test]
fn a_value_source_through_a_nested_member_is_refused() {
    let event_fields =
        "[{name: book_id, type: demo.shelf.BookId}, {name: capacity, type: Optional<Integer>}]";
    let text = control_then_refusal(
        &Shape {
            inputs: TOP_AND_WRAP,
            payload: "{book_id: input.book_id, capacity: {related: {via: input.other, field: capacity}}}",
            event_fields,
            ..Shape::default()
        },
        &Shape {
            inputs: TOP_AND_WRAP,
            payload: "{book_id: input.book_id, capacity: {related: {via: input.wrap.shelf, field: capacity}}}",
            event_fields,
            ..Shape::default()
        },
    );
    // ESS names the source's site, not the path it could not read.
    names(
        &text,
        "[undeclared_reference]",
        "outcomes.placed.payload.demo.shelf.BookPlaced.capacity",
    );
}

#[test]
fn a_member_of_a_stored_struct_is_refused() {
    let book_fields =
        "[{name: shelf_ref, type: demo.shelf.ShelfId}, {name: wrap, type: demo.shelf.Wrap}]";
    let moved = |via: &str| {
        format!(
            "  - name: demo.shelf.MoveBook
    input:
      - {{name: book_id, type: demo.shelf.BookId}}
    outcomes:
      - name: no-shelf
        when_related: {{via: {via}, exists: false}}
        error: demo.shelf.NoShelf
        summary: The stored shelf does not exist.
      - name: moved
        updates: demo.shelf.Book
        instance: book_id
        emits: [demo.shelf.BookMoved]
        payload:
          demo.shelf.BookMoved: {{book_id: input.book_id}}
        summary: The book is moved."
        )
    };
    let inputs = "      - {name: shelf_ref, type: demo.shelf.ShelfId}
      - {name: wrap, type: demo.shelf.Wrap}";
    let top = moved("shelf_ref");
    let nested = moved("wrap.shelf");
    let sets = "        sets: {shelf_ref: input.shelf_ref, wrap: input.wrap}";
    let text = control_then_refusal(
        &Shape {
            inputs,
            book_fields,
            sets,
            extra_commands: &top,
            ..Shape::default()
        },
        &Shape {
            inputs,
            book_fields,
            sets,
            extra_commands: &nested,
            ..Shape::default()
        },
    );
    assert!(
        text.lines().any(|line| line.contains("demo.shelf.MoveBook")
            && line.contains("outcomes.no-shelf.when_related")
            && line.contains("wrap.shelf")),
        "{text}"
    );
}
