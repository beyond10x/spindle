//! Where a forced external outcome is consumed: the two ways it lapses, each pinned on its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, EventRef, OutcomeRef};
use ess_conformance::{
    ConformanceTarget, Deadline, EventObservationRequest, ExternalOutcomeControl, ScenarioContext,
    ScenarioId, SemanticCommandRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_host::Answers;

const FORCING: &str = "crates/spindle-conformance/tests/fixtures/forcing";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target() -> HostTarget {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment = Environment::parse(&text).expect("it parses");
    let ir = spindle_spec::load(&root().join(FORCING))
        .expect("the forcing fixture loads")
        .ir;
    HostTarget::new(ir, spindle_conformance::in_memory(&environment))
}

fn open(target: &HostTarget, command: &str, outcome: &str) -> ScenarioContext {
    let scenario = ScenarioContext::new(
        ScenarioId::parse(&format!("{command}/outcome/{outcome}")).expect("a scenario id"),
        CorrelationId::new(format!("forcing-{outcome}")).expect("a correlation"),
    );
    target
        .begin_scenario(&scenario)
        .expect("the scenario opens");
    scenario
}

fn force(target: &HostTarget, scenario: &ScenarioContext, command: &str, outcome: &str) {
    target
        .configure_external_outcome(ExternalOutcomeControl {
            force: OutcomeRef::new(
                command.parse::<CommandRef>().expect("a command"),
                OutcomeName::new(outcome).expect("an outcome name"),
            ),
            correlation: scenario.correlation.clone(),
        })
        .expect("the target takes the forcing");
}

fn send(
    target: &HostTarget,
    scenario: &ScenarioContext,
    command: &str,
    input: &[(&str, &str)],
) -> String {
    let input = input
        .iter()
        .map(|(field, value)| ((*field).to_owned(), Node::Text((*value).to_owned())))
        .collect::<BTreeMap<_, _>>();
    target
        .execute_command(SemanticCommandRequest {
            command: command.parse::<CommandRef>().expect("a command"),
            actor: None,
            caller: None,
            input,
            correlation: scenario.correlation.clone(),
        })
        .unwrap_or_else(|error| panic!("{command} runs: {error:?}"))
        .outcome
        .map(|outcome| outcome.outcome.as_str().to_owned())
        .unwrap_or_else(|| "<undeclared>".to_owned())
}

/// One request is relayed by two bindings to two reviews in one submission. The forcing of
/// `flagged` is consumed by the first review that asks it, so exactly one of the two passes.
#[test]
fn a_forcing_is_consumed_by_the_first_invocation_that_answers_it() {
    let target = target();
    let scenario = open(&target, "forcing.desk.Review", "flagged");
    force(&target, &scenario, "forcing.desk.Review", "flagged");
    assert_eq!(
        send(
            &target,
            &scenario,
            "forcing.request.Request",
            &[("first_id", "r-1"), ("second_id", "r-2")]
        ),
        "requested"
    );
    let reviewed = target
        .observe_events(EventObservationRequest {
            event: "forcing.desk.Reviewed"
                .parse::<EventRef>()
                .expect("an event"),
            correlation: scenario.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the events are observed");
    assert_eq!(reviewed.len(), 1, "{reviewed:?}");
    target.end_scenario(&scenario).expect("the scenario closes");
}

/// A forcing lapses at the next invocation of its command even where that invocation never asks
/// its cause: closing a closed case answers `wrong-state` before `vetoed` is asked, and the next
/// open case then closes.
#[test]
fn a_forcing_lapses_at_an_invocation_that_does_not_ask_it() {
    const CLOSE: &str = "forcing.desk.CloseCase";
    let target = target();
    let scenario = open(&target, CLOSE, "vetoed");
    let case = |id| [("case_id", id)];
    for id in ["c-1", "c-2"] {
        assert_eq!(
            send(&target, &scenario, "forcing.desk.OpenCase", &case(id)),
            "opened"
        );
    }
    assert_eq!(send(&target, &scenario, CLOSE, &case("c-1")), "closed");
    force(&target, &scenario, CLOSE, "vetoed");
    assert_eq!(
        send(&target, &scenario, CLOSE, &case("c-1")),
        "wrong-state",
        "a closed case is not asked"
    );
    assert_eq!(send(&target, &scenario, CLOSE, &case("c-2")), "closed");
    target.end_scenario(&scenario).expect("the scenario closes");
}

/// The fixture whose `probe.review.Submit` has two external causes, `flagged` then `escalated`,
/// beside a default branch.
const REVIEW: &str = "crates/spindle-cli/tests/fixtures/review";

/// A target running [`REVIEW`], answering what nothing forces from the decisions file `decisions`
/// where one is given.
fn review(decisions: Option<&str>) -> HostTarget {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment = Environment::parse(&text).expect("it parses");
    let ir = spindle_spec::load(&root().join(REVIEW))
        .expect("the review fixture loads")
        .ir;
    let answers = decisions.map(|text| Answers::parse(&ir, text).expect("the decisions parse"));
    let target = HostTarget::new(ir, spindle_conformance::in_memory(&environment));
    match answers {
        Some(answers) => target.answering(answers),
        None => target,
    }
}

/// A cause nothing forces is answered by the decisions file; the scenario's forcing answers every
/// cause of its command first, until it is consumed (`story:replay-with-decisions`). Without a
/// file every cause nothing forces does not hold, as before.
#[test]
fn a_decided_cause_answers_where_nothing_is_forced() {
    const SUBMIT: &str = "probe.review.Submit";
    let send_submit = |target: &HostTarget, scenario: &ScenarioContext, id: &str| {
        let input = BTreeMap::from([
            ("submission_id".to_owned(), Node::Text(id.to_owned())),
            (
                "amount".to_owned(),
                serde_json::from_value::<Node>(serde_json::json!(5)).expect("an integer"),
            ),
        ]);
        target
            .execute_command(SemanticCommandRequest {
                command: SUBMIT.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input,
                correlation: scenario.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{SUBMIT} runs: {error:?}"))
            .outcome
            .map(|outcome| outcome.outcome.as_str().to_owned())
            .unwrap_or_else(|| "<undeclared>".to_owned())
    };

    let decided = review(Some("[\"probe.review.Submit\"]\nflagged = true\n"));
    let scenario = open(&decided, SUBMIT, "flagged");
    assert_eq!(send_submit(&decided, &scenario, "s-1"), "flagged");
    force(&decided, &scenario, SUBMIT, "escalated");
    assert_eq!(
        send_submit(&decided, &scenario, "s-2"),
        "escalated",
        "the forcing answers `flagged` before the file does"
    );
    assert_eq!(
        send_submit(&decided, &scenario, "s-3"),
        "flagged",
        "the consumed forcing leaves the file to answer"
    );
    decided
        .end_scenario(&scenario)
        .expect("the scenario closes");

    let undecided = review(None);
    let scenario = open(&undecided, SUBMIT, "submitted");
    assert_eq!(send_submit(&undecided, &scenario, "s-1"), "submitted");
    undecided
        .end_scenario(&scenario)
        .expect("the scenario closes");
}

/// A forcing lapses at an invocation a binding makes that never asks its cause: the request's
/// binding closes a closed case, which answers `wrong-state` unasked, and the next open case then
/// closes.
#[test]
fn a_forcing_lapses_at_a_bound_invocation_that_does_not_ask_it() {
    const CLOSE: &str = "forcing.desk.CloseCase";
    let target = target();
    let scenario = open(&target, CLOSE, "vetoed");
    let case = |id| [("case_id", id)];
    for id in ["c-1", "c-2"] {
        assert_eq!(
            send(&target, &scenario, "forcing.desk.OpenCase", &case(id)),
            "opened"
        );
    }
    assert_eq!(send(&target, &scenario, CLOSE, &case("c-1")), "closed");
    force(&target, &scenario, CLOSE, "vetoed");
    assert_eq!(
        send(
            &target,
            &scenario,
            "forcing.request.Request",
            &[("first_id", "c-1"), ("second_id", "x-1")]
        ),
        "requested"
    );
    assert_eq!(send(&target, &scenario, CLOSE, &case("c-2")), "closed");
    target.end_scenario(&scenario).expect("the scenario closes");
}
