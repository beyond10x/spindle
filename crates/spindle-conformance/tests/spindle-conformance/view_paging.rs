//! A paged view read through the in-process conformance target (`story:view-paging`): the page and
//! the size travel as the view's declared parameters, the answer carries
//! `SemanticViewResult::total` where the view declares `total: true`, and a page or size ESS's
//! interpreter refuses is reported unsupported, as the interpreter reports it.
//!
//! The model is `spindle-ingress`'s `tests/fixtures/served-operations`: `NotePages`
//! (`page` and `size`, `order_by: [note_id asc]`, `first_page` 0, `total: true`) and
//! `NotePagesByText` (`p` and `n`, `first_page: 1`), over the five notes [`NOTES`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{Deadline, TargetError};
use ess_conformance::{
    ConformanceTarget, ScenarioContext, ScenarioId, SemanticCommandRequest, SemanticViewRequest,
    SemanticViewResult,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::json;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const SPEC: &str = "crates/spindle-ingress/tests/fixtures/served-operations";
const WRITE: &str = "notes.pad.WriteNote";
const PAGES: &str = "notes.pad.NotePages";
const BY_TEXT: &str = "notes.pad.NotePagesByText";

/// The notes written, `(note_id, text)`.
const NOTES: [(&str, &str); 5] = [
    ("n1", "b"),
    ("n2", "a"),
    ("n3", "b"),
    ("n4", "c"),
    ("n5", "a"),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join(SPEC))
        .expect("the served-operations fixture compiles")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn context() -> ScenarioContext {
    ScenarioContext::new(
        ScenarioId::parse("notes.pad.WriteNote/authored/view-paging").expect("a scenario id"),
        CorrelationId::new("view-paging").expect("a correlation id"),
    )
}

/// Opens a scenario on `target` and writes [`NOTES`] through it.
fn written(target: &impl ConformanceTarget) {
    let context = context();
    target.begin_scenario(&context).expect("the scenario opens");
    for (id, text) in NOTES {
        target
            .execute_command(SemanticCommandRequest {
                command: WRITE.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: BTreeMap::from([
                    ("note_id".to_owned(), Node::Text(id.to_owned())),
                    ("text".to_owned(), Node::Text(text.to_owned())),
                ]),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{id} is written: {error:?}"));
    }
}

/// `view` read through `target` with the integer parameters `params`.
fn read(
    target: &impl ConformanceTarget,
    view: &str,
    params: &[(&str, i64)],
) -> Result<SemanticViewResult, TargetError> {
    target.query_view(SemanticViewRequest {
        view: ViewRef::new(view.parse().expect("a view name")),
        params: params
            .iter()
            .map(|(name, value)| {
                (
                    (*name).to_owned(),
                    serde_json::from_value(json!(value)).expect("an integer as an ESS value"),
                )
            })
            .collect(),
        consistency: QueryConsistency::Current,
        correlation: context().correlation,
        deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
    })
}

/// The `note_id` of each row of `result`, in order.
fn ids(result: &SemanticViewResult) -> Vec<String> {
    result
        .rows
        .iter()
        .map(|row| match row.get("note_id") {
            Some(Node::Text(id)) => id.clone(),
            other => panic!("a row without a text note_id: {other:?}"),
        })
        .collect()
}

#[test]
fn the_conformance_target_answers_a_page_and_its_total() {
    let target = HostTarget::new(ir(), laptop());
    written(&target);
    let page = read(&target, PAGES, &[("page", 1), ("size", 2)])
        .unwrap_or_else(|error| panic!("the page is read: {error:?}"));
    assert_eq!(ids(&page), ["n3", "n4"], "{page:?}");
    assert_eq!(page.total, Some(5), "{page:?}");

    let interpreter = Interpreted::for_model(ir());
    written(&interpreter);
    let interpreted = read(&interpreter, PAGES, &[("page", 1), ("size", 2)])
        .unwrap_or_else(|error| panic!("the interpreter reads the page: {error:?}"));
    assert_eq!(page, interpreted, "the page ESS's interpreter answers");
}

#[test]
fn the_conformance_target_reports_a_bad_page_unsupported() {
    let target = HostTarget::new(ir(), laptop());
    written(&target);
    let interpreter = Interpreted::for_model(ir());
    written(&interpreter);
    for (view, params) in [
        (PAGES, [("page", 0), ("size", 2)]),
        (BY_TEXT, [("p", 1), ("n", 2)]),
    ] {
        let good = read(&target, view, &params);
        assert!(
            good.is_ok(),
            "{view} {params:?} reads, so a refusal below is the page's: {good:?}"
        );
    }
    let cases: [(&str, Vec<(&str, i64)>); 6] = [
        (PAGES, vec![("page", 1)]),
        (PAGES, vec![("size", 2)]),
        (PAGES, vec![("page", -1), ("size", 2)]),
        (PAGES, vec![("page", 1), ("size", -2)]),
        (PAGES, vec![("page", i64::MAX), ("size", 3)]),
        (BY_TEXT, vec![("p", 0), ("n", 2)]),
    ];
    for (view, params) in cases {
        let hosted = read(&target, view, &params);
        assert!(
            matches!(hosted, Err(TargetError::Unsupported { .. })),
            "{view} {params:?} is unsupported on the host: {hosted:?}"
        );
        let interpreted = read(&interpreter, view, &params);
        assert!(
            matches!(interpreted, Err(TargetError::Unsupported { .. })),
            "{view} {params:?} is unsupported on ESS's interpreter: {interpreted:?}"
        );
    }
}

// Adversary cases (story:view-paging): page and size values a scenario may send that no query
// string carries, each answered by the host as ESS 0.99.1's interpreter answers it.

/// `view` read through `target` with the ESS values `params`, given as JSON.
fn read_nodes(
    target: &impl ConformanceTarget,
    view: &str,
    params: &[(&str, serde_json::Value)],
) -> Result<SemanticViewResult, TargetError> {
    target.query_view(SemanticViewRequest {
        view: ViewRef::new(view.parse().expect("a view name")),
        params: params
            .iter()
            .map(|(name, value)| {
                (
                    (*name).to_owned(),
                    serde_json::from_value(value.clone()).expect("an ESS value"),
                )
            })
            .collect(),
        consistency: QueryConsistency::Current,
        correlation: context().correlation,
        deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
    })
}

/// Whether two answers agree: the same rows and total, or both unsupported.
fn agree(
    hosted: &Result<SemanticViewResult, TargetError>,
    interpreted: &Result<SemanticViewResult, TargetError>,
) -> bool {
    match (hosted, interpreted) {
        (Ok(hosted), Ok(interpreted)) => hosted == interpreted,
        (Err(TargetError::Unsupported { .. }), Err(TargetError::Unsupported { .. })) => true,
        _ => false,
    }
}

/// A page and a size sent as `null`: ESS's interpreter validates a paging parameter it is given,
/// and `null` is not an `Integer`, so it reports the read unsupported.
#[test]
fn adversary_a_null_page_and_size_is_answered_as_the_interpreter_answers_it() {
    let target = HostTarget::new(ir(), laptop());
    written(&target);
    let interpreter = Interpreted::for_model(ir());
    written(&interpreter);
    let params = [("page", json!(null)), ("size", json!(null))];
    let hosted = read_nodes(&target, PAGES, &params);
    let interpreted = read_nodes(&interpreter, PAGES, &params);
    assert!(
        agree(&hosted, &interpreted),
        "page and size null: the host answers {hosted:?}, ESS's interpreter {interpreted:?}"
    );
}

/// A page sent as a fraction or as an integral float: answered as ESS's interpreter answers it.
#[test]
fn adversary_a_fractional_page_is_answered_as_the_interpreter_answers_it() {
    let target = HostTarget::new(ir(), laptop());
    written(&target);
    let interpreter = Interpreted::for_model(ir());
    written(&interpreter);
    for params in [
        [("page", json!(1.0)), ("size", json!(2))],
        [("page", json!(1.5)), ("size", json!(2))],
        [("page", json!(1)), ("size", json!(2.0))],
    ] {
        let hosted = read_nodes(&target, PAGES, &params);
        let interpreted = read_nodes(&interpreter, PAGES, &params);
        assert!(
            agree(&hosted, &interpreted),
            "{params:?}: the host answers {hosted:?}, ESS's interpreter {interpreted:?}"
        );
    }
}

/// A model of paged views alone, so no view the host refuses ends a synthesised scenario before
/// its page reads: a page and size at their defaults, a newtype page under a wire name beside an
/// `Optional<Integer>` size numbered from 1 over a descending order. (ESS synthesises no witness
/// for an `Integer` identity, and no read of a view with a filter parameter, so neither is here.)
const SHELF_SYSTEM: &str = "format: ess/34
system: shelf
version: v1
summary: Books paged.
domains:
  - shelf.book
components:
  - component: shelf
    summary: Holds books.
    owns:
      domains:
        - shelf.book
    accepts:
      commands:
        - shelf.book.AddBook
    publishes:
      events:
        - shelf.book.BookAdded
    reached_by: network
";

const SHELF_DOMAIN: &str = "domain: shelf.book
summary: Books, paged.
types:
  - name: shelf.book.PageNo
    kind: newtype
    of: Integer
  - name: shelf.book.BookId
    kind: newtype
    of: String
entities:
  - name: shelf.book.Book
    identity:
      name: book_id
      type: shelf.book.BookId
    fields:
      - name: title
        type: String
      - name: stars
        type: Integer
    lifecycle:
      initial: Kept
      states: [Kept]
      terminal: [Kept]
commands:
  - name: shelf.book.AddBook
    input:
      - name: book_id
        type: shelf.book.BookId
      - name: title
        type: String
      - name: stars
        type: Integer
    outcomes:
      - name: added
        creates: shelf.book.Book
        instance: book_id
        sets:
          title: input.title
          stars: input.stars
        emits: [shelf.book.BookAdded]
        payload:
          shelf.book.BookAdded: {book_id: input.book_id}
        summary: The book is kept.
events:
  - name: shelf.book.BookAdded
    fields:
      - name: book_id
        type: shelf.book.BookId
views:
  - name: shelf.book.BookPages
    source: shelf.book.Book
    consistency: read_your_writes
    params:
      - {name: page, type: Integer}
      - {name: size, type: Integer}
    order_by: [book_id asc]
    paging: {page: page, size: size, total: true}
    fields:
      - {name: book_id, type: shelf.book.BookId}
      - {name: title, type: String}
  - name: shelf.book.ByStarsDesc
    source: shelf.book.Book
    consistency: read_your_writes
    params:
      - {name: page, type: shelf.book.PageNo, wire: pageNumber}
      - {name: size, type: Optional<Integer>, wire: pageSize}
    order_by: [stars desc]
    paging: {page: page, size: size, first_page: 1, total: true}
    fields:
      - {name: book_id, type: shelf.book.BookId}
      - {name: title, type: String}
      - {name: stars, type: Integer}
";

/// The page reads ESS 0.99.1 synthesises for paged views (suite/26, `synthesize/paging.rs`: a
/// first page, the page after it, and a page larger than every row), run against the host: every
/// check about a view ends on the host as it ends on ESS's interpreter, scenario by scenario.
#[test]
fn adversary_the_synthesised_page_scenarios_pass_on_the_host() {
    use ess_conformance::AdmittedSuite;
    let started = std::time::Instant::now();
    let spec = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("adversary-view-paging-{}", std::process::id()));
    std::fs::create_dir_all(spec.join("domains")).expect("the spec directory");
    std::fs::write(spec.join("system.yaml"), SHELF_SYSTEM).expect("the system");
    std::fs::write(spec.join("domains/book.yaml"), SHELF_DOMAIN).expect("the domain");
    let loaded = spindle_spec::load(&spec);
    std::fs::remove_dir_all(&spec).expect("the spec directory is removed");
    let loaded = loaded.unwrap_or_else(|error| panic!("the shelf loads: {error:?}"));
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let hosted =
        spindle_conformance::execute(&suite, &HostTarget::new(loaded.ir.clone(), laptop()));
    let interpreted =
        spindle_conformance::execute(&suite, &Interpreted::for_model(loaded.ir.clone()));
    // The checks of `result` about a view of the shelf, each with its status and diagnostic, in
    // the order they ran.
    let paged = |result: &ess_conformance::ScenarioResult| {
        result
            .checks
            .iter()
            .filter(|check| check.about.contains("view") && check.about.contains("shelf.book."))
            .map(|check| {
                format!(
                    "{} -> {:?} {:?}",
                    check.about, check.status, check.diagnostic
                )
            })
            .collect::<Vec<_>>()
    };
    let mut compared = 0;
    let mut differ = Vec::new();
    for expected in &interpreted.executed.scenarios {
        let wanted = paged(expected);
        if wanted.is_empty() {
            continue;
        }
        let got = hosted
            .executed
            .scenarios
            .iter()
            .find(|result| result.scenario == expected.scenario)
            .map(paged)
            .unwrap_or_default();
        let brief = |checks: &[String]| {
            checks
                .iter()
                .map(|check| {
                    let (about, rest) = check.split_once(" -> ").unwrap_or((check, ""));
                    format!("{about} -> {}", rest.split(' ').next().unwrap_or(""))
                })
                .collect::<Vec<_>>()
        };
        if brief(&got) != brief(&wanted) {
            differ.push(format!(
                "{}:\n  host        {got:#?}\n  interpreter {:#?}",
                expected.scenario,
                brief(&wanted)
            ));
        }
        compared += wanted.len();
    }
    assert!(
        compared > 0,
        "some synthesised check is about a paged view: {:#?} {:#?}",
        interpreted
            .executed
            .scenarios
            .iter()
            .map(|result| (
                result.scenario.to_string(),
                result.status,
                result
                    .checks
                    .iter()
                    .map(|check| &check.about)
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        synthesis.refusals
    );
    assert!(
        differ.is_empty(),
        "{} of the scenarios reading a paged view differ:\n{}",
        differ.len(),
        differ.join("\n")
    );
    eprintln!(
        "{compared} paged checks compared in {:?}",
        started.elapsed()
    );
}
