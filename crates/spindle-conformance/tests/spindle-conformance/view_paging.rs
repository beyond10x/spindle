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
