//! Adversary cases for story:actor-command-grants, held to ESS 0.86.0's interpreter.
//!
//! ESS's interpreter refuses an ungranted actor before it takes what a scenario forced
//! (`Interpreted::decide`: `command_grant`, then `Caller::bind`, then `scenario.forced.take()`),
//! so a forced external outcome survives a send the grant refuses and holds for the next granted
//! send of the same command.
//!
//! The fixture `tests/fixtures/adversary-grants` declares `Clerk`, who may send `Review`, and
//! `Guest`, who may only send `Wave`; `Review` has an `external:` outcome `flagged`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{ActorRef, CommandRef, OutcomeRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    ConformanceTarget, ExternalOutcomeControl, ScenarioContext, ScenarioId, SemanticCommandRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const SPEC: &str = "crates/spindle-conformance/tests/fixtures/adversary-grants";
const REVIEW: &str = "gate.desk.Review";
const CLERK: &str = "gate.desk.Clerk";
const GUEST: &str = "gate.desk.Guest";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join(SPEC))
        .expect("the adversary-grants fixture compiles")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn send(
    target: &impl ConformanceTarget,
    context: &ScenarioContext,
    actor: &str,
    review_id: &str,
) -> String {
    let result = target.execute_command(SemanticCommandRequest {
        command: REVIEW.parse::<CommandRef>().expect("a command"),
        actor: Some(ActorRef::new(actor.parse().expect("an actor name"))),
        caller: None,
        input: [("review_id".to_owned(), Node::Text(review_id.to_owned()))]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
        correlation: context.correlation.clone(),
    });
    match result {
        Ok(result) => format!(
            "{}{}",
            result.outcome.map_or_else(
                || "<undeclared>".to_owned(),
                |o| o.outcome.as_str().to_owned()
            ),
            result
                .error
                .map(|error| format!(" with {}", error.error))
                .unwrap_or_default()
        ),
        Err(error) => error.to_string(),
    }
}

/// Forces `Review/flagged`, sends `Review` as the guest (refused, not granted), then as the clerk.
fn forced_then_refused_then_granted(target: &impl ConformanceTarget) -> Vec<String> {
    let context = ScenarioContext::new(
        ScenarioId::parse(&format!("{REVIEW}/outcome/flagged")).expect("a scenario id"),
        CorrelationId::new("adversary-grants").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    target
        .configure_external_outcome(ExternalOutcomeControl {
            force: OutcomeRef::new(
                REVIEW.parse::<CommandRef>().expect("a command"),
                OutcomeName::new("flagged").expect("an outcome name"),
            ),
            correlation: context.correlation.clone(),
        })
        .expect("the target takes the forcing");
    let answers = vec![
        send(target, &context, GUEST, "r1"),
        send(target, &context, CLERK, "r1"),
    ];
    let _ = target.end_scenario(&context);
    answers
}

/// A send the grant refuses never reaches the decision, so it must not consume or lapse what the
/// scenario forced for its command: the clerk's send that follows is `flagged`, as on ESS.
#[test]
fn a_grant_refusal_does_not_lapse_a_forced_outcome() {
    let ess = forced_then_refused_then_granted(&Interpreted::for_model(ir()));
    let spindle = forced_then_refused_then_granted(&HostTarget::new(ir(), laptop()));
    assert_eq!(
        ess,
        vec![
            format!("refused as not granted to `{GUEST}`"),
            "flagged with gate.desk.Flagged".to_owned(),
        ],
        "ESS's interpreter keeps the forcing past the refused send"
    );
    assert_eq!(
        spindle, ess,
        "the host answers the refused and the granted send as ESS's interpreter does"
    );
}
