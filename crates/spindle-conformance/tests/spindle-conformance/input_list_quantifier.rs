//! The suite ESS 0.99.2 synthesises for a refusal that quantifies over an input list
//! (`story:ess-pin-0-99-2`), run against the in-process host. ESS 0.99.2 sends the accepting
//! branch beside such a refusal a list of several elements, none of which meets the quantifier,
//! where 0.99.1 sent it only an empty list; and it puts the refusal's `trimmed` witness, the
//! literal between `\v\f` and `\r\n`, on the element that decides the refusal, where 0.99.1 put
//! it on element 0, a filler, and left the deciding element beside it.
//!
//! The model is the neutral `tests/fixtures/input-list-quantifier`: `PackBox` refuses a box when
//! one of its labels has a text that is blank once trimmed. The labels are a list of a struct,
//! not of `String`: ESS 0.99.2 moves a witness at `<list>.0.<member>` only, and the suite it
//! synthesises for a `List<String>` is the one 0.99.1 synthesises.

use std::path::Path;

use ess_conformance::interpret::Interpreted;
use ess_conformance::{AdmittedSuite, Runner, ScenarioStep, ScenarioValue, Status, Synthesis};
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_spec::Specification;

const FIXTURE: &str = "tests/fixtures/input-list-quantifier";
const PACKED: &str = "shelf.box.PackBox/outcome/packed";
const BLANK_LABEL: &str = "shelf.box.PackBox/outcome/blank-label";

/// The literal between `\v\f` and `\r\n` of a `trimmed` guard's witnesses
/// (`ess_conformance::witness::trim_witnesses`).
const VERTICAL: &str = "\u{b}\u{c}\r\n";

fn loaded() -> Specification {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
}

fn laptop() -> Environment {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn synthesized(specification: &Specification) -> Synthesis {
    let synthesis = spindle_conformance::synthesize(specification).expect("the suite synthesises");
    assert!(
        synthesis.refusals.is_empty(),
        "ESS refuses no scenario of {FIXTURE}: {:#?}",
        synthesis.refusals
    );
    synthesis
}

/// Each `labels` list the scenario `id` sends, with the outcome the step after it expects.
fn sent_labels(synthesis: &Synthesis, id: &str) -> Vec<(Vec<Node>, String)> {
    let (_, scenario) = synthesis
        .suite
        .scenarios
        .iter()
        .find(|(key, _)| key.to_string() == id)
        .unwrap_or_else(|| panic!("{id} is in the suite"));
    let mut sent = Vec::new();
    for (at, step) in scenario.steps.iter().enumerate() {
        let ScenarioStep::ExecuteCommand { input, .. } = step else {
            continue;
        };
        let Some(ScenarioValue::Literal {
            value: Node::Seq(list),
        }) = input.get("labels")
        else {
            continue;
        };
        let expected = scenario.steps[at + 1..]
            .iter()
            .find_map(|step| match step {
                ScenarioStep::ExpectOutcome { outcome } => Some(outcome.outcome.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        sent.push((list.clone(), expected));
    }
    sent
}

/// The text of a label, where it is one.
fn text(label: &Node) -> Option<&str> {
    match label {
        Node::Map(members) => match members.get("text") {
            Some(Node::Text(text)) => Some(text),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `text` is empty once the six ASCII whitespace characters a `trimmed` guard removes are
/// trimmed from it.
fn trims_blank(text: &str) -> bool {
    text.trim_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r'])
        .is_empty()
}

/// The status of every scenario of `synthesis` on ESS's interpreter and on the host, in suite
/// order.
type Statuses = Vec<(String, Status)>;

fn statuses(specification: &Specification, synthesis: &Synthesis) -> (Statuses, Statuses) {
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let interpreted = Runner::for_suite(suite.suite())
        .run_admitted(&suite, &Interpreted::for_model(specification.ir.clone()))
        .into_report()
        .scenarios
        .into_iter()
        .map(|run| (run.scenario.to_string(), run.status))
        .collect();
    let run =
        spindle_conformance::execute(&suite, &HostTarget::new(specification.ir.clone(), laptop()));
    let hosted = run
        .executed
        .scenarios
        .iter()
        .map(|result| (result.scenario.to_string(), result.status))
        .collect();
    (interpreted, hosted)
}

#[test]
fn the_accepting_scenario_of_an_input_list_quantifier_sends_several_elements_and_passes() {
    let specification = loaded();
    let synthesis = synthesized(&specification);
    let sent = sent_labels(&synthesis, PACKED);
    let several: Vec<&Vec<Node>> = sent
        .iter()
        .filter(|(list, expected)| list.len() >= 2 && expected == "packed")
        .map(|(list, _)| list)
        .collect();
    assert!(
        !several.is_empty(),
        "{PACKED} sends a list of two or more labels and expects `packed`, got {sent:?}"
    );
    for list in &several {
        assert!(
            list.iter().filter_map(text).all(|text| !trims_blank(text)),
            "no label {PACKED} sends is blank once trimmed: {list:?}"
        );
    }
    let (interpreted, hosted) = statuses(&specification, &synthesis);
    assert_eq!(
        hosted, interpreted,
        "each scenario has the status on the host it has on ESS's interpreter"
    );
    assert!(
        hosted.iter().any(|(id, _)| id == PACKED),
        "{PACKED} executes: {hosted:?}"
    );
    for (id, status) in &hosted {
        assert_eq!(*status, Status::Passed, "{id} passes on the host");
    }
}

#[test]
fn a_trimmed_witness_on_the_deciding_element_is_answered_as_ess_answers_it() {
    let specification = loaded();
    let synthesis = synthesized(&specification);
    let sent = sent_labels(&synthesis, BLANK_LABEL);
    let decided_by_it = sent.iter().any(|(list, expected)| {
        let blank: Vec<&str> = list
            .iter()
            .filter_map(text)
            .filter(|text| trims_blank(text))
            .collect();
        list.len() >= 2 && expected == "blank-label" && blank == [VERTICAL]
    });
    assert!(
        decided_by_it,
        "{BLANK_LABEL} sends a list of two or more labels whose one label blank once trimmed is \
         \\v\\f..\\r\\n, got {sent:?}"
    );
    let (interpreted, hosted) = statuses(&specification, &synthesis);
    let on = |statuses: &Statuses| {
        statuses
            .iter()
            .find(|(id, _)| id == BLANK_LABEL)
            .map(|(_, status)| *status)
    };
    assert_eq!(
        on(&hosted),
        on(&interpreted),
        "{BLANK_LABEL} has the status on the host it has on ESS's interpreter"
    );
    assert_eq!(
        on(&hosted),
        Some(Status::Passed),
        "{BLANK_LABEL} passes on the host"
    );
}
