//! Adversary cases for `deletes:` (story kernel-delete-outcomes), run by the in-process host and
//! by ESS 0.87.0's interpreter on the same neutral fixtures.
//!
//! `tests/fixtures/adversary-deletes` opens and removes desks in `room`, and a binding drops the
//! desk's ledger entry in `ledger` when its desk is removed: a deletion inside a bound,
//! multi-component run. A book names its desk; a removed desk is the missing related row of a
//! later command. `DiscardBook` declares only its deleting outcome, so an unknown book is
//! answered by no declared branch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, Deadline, ScenarioContext, ScenarioId,
    SemanticCommandRequest, SemanticViewRequest, Status,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const STUDY: &str = "crates/spindle-conformance/tests/fixtures/adversary-deletes";
const BOOKS: &str = "crates/spindle-kernel/tests/fixtures/deletes";
const SHELF: &str = "crates/spindle-cli/tests/fixtures/kernel-delete";
const VIEWS: [&str; 3] = [
    "study.room.Desks",
    "study.room.Books",
    "study.ledger.Entries",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(path: &str) -> EssIr {
    spindle_spec::load(&root().join(path))
        .unwrap_or_else(|error| panic!("{path} loads: {error}"))
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// What one target answered for each step (outcome, or `<undeclared>`; error), and each view's
/// rows after the last step.
#[derive(Debug, PartialEq)]
struct Observed {
    answers: Vec<(String, String, Option<String>)>,
    rows: BTreeMap<String, Vec<BTreeMap<String, Node>>>,
}

fn steps() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "study.room.OpenDesk",
            vec![("desk_id", "d1"), ("label", "north")],
        ),
        (
            "study.room.OpenDesk",
            vec![("desk_id", "d2"), ("label", "south")],
        ),
        (
            "study.room.ShelveBook",
            vec![("book_id", "b1"), ("home", "d1")],
        ),
        ("study.room.RemoveDesk", vec![("desk_id", "d1")]),
        ("study.room.CheckBook", vec![("book_id", "b1")]),
        (
            "study.room.ShelveBook",
            vec![("book_id", "b2"), ("home", "d1")],
        ),
        ("study.room.CloseDesk", vec![("desk_id", "d1")]),
        ("study.room.RemoveDesk", vec![("desk_id", "d1")]),
        ("study.room.DiscardBook", vec![("book_id", "b9")]),
        ("study.room.DiscardBook", vec![("book_id", "b1")]),
        ("study.room.DiscardBook", vec![("book_id", "b1")]),
        (
            "study.room.OpenDesk",
            vec![("desk_id", "d1"), ("label", "north again")],
        ),
        ("study.room.CloseDesk", vec![("desk_id", "d1")]),
        ("study.room.RemoveDesk", vec![("desk_id", "d1")]),
        ("study.room.RemoveDesk", vec![("desk_id", "d2")]),
        (
            "study.room.OpenDesk",
            vec![("desk_id", "d1"), ("label", "third")],
        ),
    ]
}

fn observe(target: &impl ConformanceTarget) -> Observed {
    let scenario = ScenarioContext::new(
        ScenarioId::parse("study.room.RemoveDesk/authored/adversary-deletes")
            .expect("a scenario id"),
        CorrelationId::new("adversary-deletes").expect("a correlation"),
    );
    target
        .begin_scenario(&scenario)
        .expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps() {
        let input = input
            .into_iter()
            .map(|(field, value)| (field.to_owned(), Node::Text(value.to_owned())))
            .collect();
        let result = target
            .execute_command(SemanticCommandRequest {
                command: command.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input,
                correlation: scenario.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
        answers.push((
            command.to_owned(),
            result
                .outcome
                .map(|outcome| outcome.outcome.as_str().to_owned())
                .unwrap_or_else(|| "<undeclared>".to_owned()),
            result.error.map(|error| error.error.to_string()),
        ));
    }
    let mut rows = BTreeMap::new();
    for view in VIEWS {
        let mut held: Vec<BTreeMap<String, Node>> = target
            .query_view(SemanticViewRequest {
                view: ViewRef::new(view.parse().expect("a view name")),
                params: BTreeMap::new(),
                consistency: QueryConsistency::Current,
                correlation: scenario.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("{view} reads: {error:?}"))
            .rows
            .into_iter()
            .map(|row| row.into_iter().collect())
            .collect();
        held.sort_by_key(|row| format!("{row:?}"));
        rows.insert(view.to_owned(), held);
    }
    target.end_scenario(&scenario).expect("the scenario closes");
    Observed { answers, rows }
}

/// A desk removed in `room` drops its ledger entry in `ledger` through a binding; the removed desk
/// is then the missing related row of `CheckBook` and `ShelveBook`; the desk's identity is opened
/// again (and its entry kept again) after each removal; `DiscardBook`, which declares only its
/// deleting branch, is sent an identity no book carries and a removed one. Every answer and every
/// view row agrees with ESS's interpreter.
#[test]
fn deletes_in_a_bound_multi_component_run_agree_with_the_interpreter() {
    let ir = load(STUDY);
    let ess = observe(&Interpreted::for_model(ir.clone()));
    let spindle = observe(&HostTarget::new(ir, laptop()));
    for (index, (ess, spindle)) in ess.answers.iter().zip(&spindle.answers).enumerate() {
        assert_eq!(spindle, ess, "step {index}: spindle answered differently");
    }
    assert_eq!(spindle, ess);
}

/// Every scenario ESS synthesises for each fixture with a `deletes:` outcome, that the host
/// executes and the interpreter itself can carry out (its status is not `Error`), ends with the
/// status the interpreter gives it.
#[test]
fn the_synthesised_delete_scenarios_answer_as_the_interpreter() {
    for fixture in [BOOKS, SHELF, STUDY] {
        let specification = spindle_spec::load(&root().join(fixture))
            .unwrap_or_else(|error| panic!("{fixture} loads: {error}"));
        let synthesis =
            spindle_conformance::synthesize(&specification).expect("the suite synthesises");
        let ir = specification.ir;
        let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
        let statuses = |run: &spindle_conformance::Run| {
            run.report()
                .scenarios
                .iter()
                .map(|result| (result.scenario.to_string(), result.status))
                .collect::<BTreeMap<_, _>>()
        };
        let ess_run = spindle_conformance::execute(&suite, &Interpreted::for_model(ir.clone()));
        let spindle_run = spindle_conformance::execute(&suite, &HostTarget::new(ir, laptop()));
        let ess = statuses(&ess_run);
        let spindle = statuses(&spindle_run);
        let divergent = spindle
            .iter()
            .filter(|(_, status)| **status != Status::Unsupported)
            .filter(|(scenario, _)| ess.get(*scenario) != Some(&Status::Error))
            .filter(|(scenario, status)| ess.get(*scenario) != Some(status))
            .collect::<Vec<_>>();
        let checks = |run: &spindle_conformance::Run| {
            run.report()
                .scenarios
                .iter()
                .filter(|result| {
                    divergent
                        .iter()
                        .any(|(id, _)| **id == result.scenario.to_string())
                })
                .flat_map(|result| {
                    result
                        .checks
                        .iter()
                        .filter(|check| check.status != Status::Passed)
                        .map(move |check| {
                            format!(
                                "{}: {check} {}",
                                result.scenario,
                                check
                                    .diagnostic
                                    .as_ref()
                                    .map(ToString::to_string)
                                    .unwrap_or_default()
                            )
                        })
                })
                .collect::<Vec<_>>()
        };
        assert!(
            divergent.is_empty(),
            "{fixture}\ndivergent: {divergent:?}\ness checks: {:#?}\nspindle checks: {:#?}",
            checks(&ess_run),
            checks(&spindle_run)
        );
        let deleting = spindle
            .iter()
            .filter(|(scenario, status)| {
                **status == Status::Passed
                    && (scenario.contains("Remove")
                        || scenario.contains("Discard")
                        || scenario.contains("Drop"))
            })
            .count();
        assert!(
            deleting > 0,
            "{fixture}: the host passes some scenario of a deleting command: {spindle:?}"
        );
    }
}
