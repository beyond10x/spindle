//! An `Optional` view field whose source row holds nothing
//! (`story:view-rows-absent-optional-fields`): read through the conformance target
//! ([`HostTarget`]'s `query_view`) it is answered as ESS's interpreter answers it, `null` unless
//! the field carries `presence: omitted_when_absent`, `null` through a reference that names no
//! row, and a stored `null` kept as `null`; read through a served view ([`Host::listed_with`]) it
//! keeps ESS's published contract, which leaves such a field out.
//!
//! `tests/fixtures/absent-view-fields` keeps shelves, each with an optional label, and books, each
//! with an optional `note` (no presence), `remark` (`null_when_absent`), `aside`
//! (`omitted_when_absent`) and `shelf_id`, a reference to a shelf. `Books` projects every field and
//! the label and state of the shelf, read through the reference; each field projected from the
//! book carries the presence of the book's field of its name, and the two read through the
//! reference carry none.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, EntityRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::EntitySetupRequest;
use ess_conformance::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_domain::entity::StateName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_host::{Host, SystemClock};

const FIXTURE: &str = "tests/fixtures/absent-view-fields";
const BOOKS: &str = "ledger.books.Books";

/// The rows each view read answered, by view name.
type Read = BTreeMap<String, Vec<Map<String, Value>>>;

fn ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
        .ir
}

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn object(value: &Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object: {value}");
    };
    object.clone()
}

/// Establishes each book (`identity`, stored fields), sends `commands` in order, each of which
/// must take an outcome, and answers the rows of every view of `views`, a field held as `null` kept
/// as `null`: one scenario on `target`.
fn rows<T: ConformanceTarget>(
    target: &T,
    books: &[(&str, Value)],
    commands: &[(&str, Value)],
    views: &[&str],
) -> Read {
    let context = ScenarioContext::new(
        "ledger.books/authored/absent-view-fields"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("absent-view-fields").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    for (identity, fields) in books {
        target
            .establish_entity(EntitySetupRequest {
                entity: "ledger.books.Book".parse::<EntityRef>().expect("an entity"),
                identity: Node::Text((*identity).to_owned()),
                fields: object(fields)
                    .into_iter()
                    .map(|(field, value)| (field, node(value)))
                    .collect(),
                state: StateName::new("Kept").expect("a state"),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("book {identity} is established: {error:?}"));
    }
    for (command, input) in commands {
        target
            .execute_command(SemanticCommandRequest {
                command: command.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: object(input)
                    .into_iter()
                    .map(|(field, value)| (field, node(value)))
                    .collect(),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
    }
    let read = views
        .iter()
        .map(|view| {
            let rows = target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: BTreeMap::new(),
                    consistency: QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
                })
                .unwrap_or_else(|error| panic!("{view} reads: {error:?}"))
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|(field, value)| (field.clone(), json_of(value)))
                        .collect()
                })
                .collect();
            ((*view).to_owned(), rows)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    read
}

/// The same scenario on ESS's interpreter, then through the conformance target.
fn both(books: &[(&str, Value)], commands: &[(&str, Value)], views: &[&str]) -> (Read, Read) {
    let ir = ir();
    (
        rows(&Interpreted::for_model(ir.clone()), books, commands, views),
        rows(&HostTarget::new(ir, laptop()), books, commands, views),
    )
}

fn shelf(id: &str, label: Option<&str>) -> (&'static str, Value) {
    let mut input = json!({"shelf_id": id});
    if let Some(label) = label {
        input["label"] = json!(label);
    }
    ("ledger.books.AddShelf", input)
}

fn book(input: Value) -> (&'static str, Value) {
    ("ledger.books.AddBook", input)
}

#[test]
fn an_optional_field_never_written_is_answered_as_ess_answers_it() {
    let (ess, spindle) = both(&[], &[book(json!({"book_id": "b1"}))], &[BOOKS]);
    assert_eq!(
        Value::Array(ess[BOOKS].iter().cloned().map(Value::Object).collect()),
        json!([{
            "book_id": "b1",
            "note": null,
            "remark": null,
            "shelf_id": null,
            "shelf_label": null,
            "shelf_state": null,
        }]),
        "ESS's interpreter answers every unwritten field `null` but the `omitted_when_absent` one"
    );
    assert_eq!(spindle, ess, "the target answers the row ESS answers");
}

#[test]
fn view_rows_agree_with_the_interpreter_on_absent_fields() {
    let books = [(
        "b5",
        json!({"note": null, "remark": null, "shelf_id": null}),
    )];
    let commands = [
        shelf("s1", Some("poetry")),
        shelf("s2", None),
        book(json!({
            "book_id": "b1",
            "note": "signed",
            "remark": "worn",
            "aside": "gift",
            "shelf_id": "s1",
        })),
        book(json!({"book_id": "b2"})),
        book(json!({"book_id": "b3", "aside": "loaned", "shelf_id": "s2"})),
        book(json!({"book_id": "b4", "note": "lost", "shelf_id": "s9"})),
    ];
    let (ess, spindle) = both(&books, &commands, &[BOOKS]);
    assert_eq!(
        ess[BOOKS].len(),
        5,
        "ESS's interpreter answers every book: {ess:?}"
    );
    assert!(
        ess.values()
            .flatten()
            .any(|row| row.values().any(Value::is_null)),
        "ESS's interpreter answers some field `null`: {ess:?}"
    );
    assert_eq!(spindle, ess, "every row agrees with the interpreter's");
}

#[test]
fn a_served_view_row_still_leaves_an_unwritten_optional_field_out() {
    let mut host = Host::new(ir(), &laptop(), Arc::new(SystemClock), Path::new(""))
        .expect("the host admits the fixture");
    host.submit("ledger.books.AddBook", object(&json!({"book_id": "b1"})))
        .expect("the book is added");
    let listed = host
        .listed_with(BOOKS, &Map::new())
        .expect("the view reads");
    assert_eq!(
        listed.rows,
        [object(&json!({"book_id": "b1", "remark": null}))],
        "a served row leaves out every unwritten field but the `null_when_absent` one"
    );
}
