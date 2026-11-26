//! A guard over an `Optional` field of the held instance, or of a related row read through
//! `when_related:`, that was never written (`story:guard-over-absent-row-field`).
//!
//! The fixture `tests/fixtures/absent-row-field` opens tickets and agents with optional `label`,
//! `rank` and `flagged` fields, each written or not. `Review` is refused `held-refused` where its
//! `when_subject:` guard over the ticket holds; `Assign` is refused `agent-refused` where its
//! `when_related:` guard over the named agent holds. Each case below writes one guard form into
//! both. An absent field binds no fact, so ESS 0.99.1's interpreter reads a comparison with it as
//! `Unknown` and selects no outcome, `defined(field)` as `false`, and `not` as Kleene negation; the
//! kernel answers every request as the interpreter does, and its `undecidable` refusal names the
//! guard and the absent field and suggests `defined(<field>)`.

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, execute};

use super::related_guards::object;

const OPEN_AGENT: &str = "desk.ticket.OpenAgent";
const OPEN_TICKET: &str = "desk.ticket.OpenTicket";
const REVIEW: &str = "desk.ticket.Review";
const ASSIGN: &str = "desk.ticket.Assign";

const BARE_AGENT: &str = "a9e70000-0000-4000-8000-000000000001";
const FULL_AGENT: &str = "a9e70000-0000-4000-8000-000000000002";
const BARE_TICKET: &str = "71c70000-0000-4000-8000-000000000001";
const FULL_TICKET: &str = "71c70000-0000-4000-8000-000000000002";

/// The guard the fixture writes in both `Review` and `Assign`, replaced by each case.
const GUARD: &str = "predicate: label == mine";

/// Each guard form, and what the kernel and the interpreter answer for a review of the ticket
/// whose fields were never written and for an assignment to the agent whose fields were never
/// written: the refusal the guard selects, the outcome after it, or `None` where no outcome is
/// selected.
const FORMS: &[(&str, Option<&str>, Option<&str>)] = &[
    ("label == mine", None, None),
    ("label != mine", None, None),
    ("{label: {any_of: [mine, theirs]}}", None, None),
    ("{label: {none_of: [mine, theirs]}}", None, None),
    ("rank > 3", None, None),
    ("rank <= 3", None, None),
    ("flagged", None, None),
    ("defined(label)", Some("reviewed"), Some("assigned")),
    (
        "not defined(label)",
        Some("held-refused"),
        Some("agent-refused"),
    ),
    ("{not: label == mine}", None, None),
    (
        "{all: [defined(label), label == mine]}",
        Some("reviewed"),
        Some("assigned"),
    ),
    (
        "{any: [not defined(label), label == mine]}",
        Some("held-refused"),
        Some("agent-refused"),
    ),
];

/// The fixture with `form` as the guard of both `Review` and `Assign`, in a directory of its own.
fn guarded(form: &str) -> (tempfile::TempDir, spindle_spec::EssIr) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/absent-row-field");
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    std::fs::copy(fixture.join("system.yaml"), dir.path().join("system.yaml"))
        .expect("the system copies");
    let text =
        std::fs::read_to_string(fixture.join("domains/ticket.yaml")).expect("the domain reads");
    assert_eq!(text.matches(GUARD).count(), 2, "both guards are written");
    std::fs::write(
        dir.path().join("domains/ticket.yaml"),
        text.replace(GUARD, &format!("predicate: {form}")),
    )
    .expect("the variant writes");
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the fixture guarded by `{form}` compiles: {error}"))
        .ir;
    (dir, ir)
}

/// An agent and a ticket with no optional field written, one of each with every field written,
/// then a review of each ticket and an assignment of the full ticket to each agent.
fn steps() -> Vec<(&'static str, Value)> {
    let full = json!({"label": "mine", "rank": 5, "flagged": true});
    let with = |mut id: Value| {
        id.as_object_mut()
            .expect("an object")
            .extend(full.as_object().expect("an object").clone());
        id
    };
    vec![
        (OPEN_AGENT, json!({"agent_id": BARE_AGENT})),
        (OPEN_AGENT, with(json!({"agent_id": FULL_AGENT}))),
        (OPEN_TICKET, json!({"ticket_id": BARE_TICKET})),
        (OPEN_TICKET, with(json!({"ticket_id": FULL_TICKET}))),
        (REVIEW, json!({"ticket_id": BARE_TICKET, "note": "first"})),
        (REVIEW, json!({"ticket_id": FULL_TICKET, "note": "first"})),
        (
            ASSIGN,
            json!({"ticket_id": FULL_TICKET, "agent_id": BARE_AGENT}),
        ),
        (
            ASSIGN,
            json!({"ticket_id": FULL_TICKET, "agent_id": FULL_AGENT}),
        ),
    ]
}

/// The step that reviews the ticket with no field written.
const BARE_REVIEW: usize = 4;
/// The step that assigns to the agent with no field written.
const BARE_ASSIGN: usize = 6;

/// What the kernel answers for each step: the outcome, or the refusal.
fn spindled(
    ir: &spindle_spec::EssIr,
    steps: &[(&str, Value)],
) -> Vec<Result<String, ExecuteError>> {
    let mut store = MemoryStore::default();
    steps
        .iter()
        .map(|(command, input)| {
            execute(
                ir,
                &mut store,
                &mut NoExternals,
                command,
                &object(input.clone()),
            )
            .map(|executed| executed.outcome)
        })
        .collect()
}

/// What ESS's interpreter answers for each step, in one scenario: the outcome, or its refusal.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<Result<String, String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "desk.ticket/authored/absent-row-field"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("absent-row-field").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    steps
        .iter()
        .map(|(command, input)| {
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: object(input.clone())
                        .iter()
                        .map(|(field, value)| {
                            (
                                field.clone(),
                                serde_json::from_value(value.clone()).expect("a node"),
                            )
                        })
                        .collect(),
                    correlation: context.correlation.clone(),
                })
                .map(|executed| {
                    executed
                        .outcome
                        .map(|outcome| outcome.outcome.as_str().to_owned())
                        .unwrap_or_default()
                })
                .map_err(|error| error.to_string())
        })
        .collect()
}

/// The kernel's answers to [`steps`] under `form`, each checked against the interpreter's: the
/// same outcome, or an `undecidable` refusal where the interpreter finds the same guard
/// `Unknown`.
fn agreed(form: &str) -> Vec<Result<String, ExecuteError>> {
    let (_dir, ir) = guarded(form);
    let steps = steps();
    let ours = spindled(&ir, &steps);
    let theirs = interpreted(&ir, &steps);
    assert_eq!(ours.len(), theirs.len());
    for (step, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => assert_eq!(ours, theirs, "`{form}`, step {step}"),
            (Err(ExecuteError::Undecidable { outcome, .. }), Err(theirs)) => assert!(
                theirs.contains(&format!("`{outcome}` is Unknown")),
                "`{form}`, step {step}: the interpreter refuses the same guard: {theirs}"
            ),
            _ => panic!(
                "`{form}`, step {step}: the kernel answers {ours:?}, the interpreter {theirs:?}"
            ),
        }
    }
    ours
}

/// The outcome a step answered, or `None` where it is refused as undecidable.
fn selected(answer: &Result<String, ExecuteError>) -> Option<&str> {
    match answer {
        Ok(outcome) => Some(outcome),
        Err(ExecuteError::Undecidable { .. }) => None,
        Err(error) => panic!("only an undecidable guard selects nothing: {error}"),
    }
}

#[test]
fn a_guard_over_an_absent_optional_field_decides_as_the_interpreter() {
    for (form, review, _) in FORMS {
        let ours = agreed(form);
        assert_eq!(
            selected(&ours[BARE_REVIEW]),
            *review,
            "`{form}` over the ticket's absent fields"
        );
        assert!(
            selected(&ours[BARE_REVIEW + 1]).is_some(),
            "`{form}` over the ticket's written fields is decided: {:?}",
            ours[BARE_REVIEW + 1]
        );
    }
}

#[test]
fn a_related_guard_over_an_absent_optional_field_decides_as_the_interpreter() {
    for (form, _, assign) in FORMS {
        let ours = agreed(form);
        assert_eq!(
            selected(&ours[BARE_ASSIGN]),
            *assign,
            "`{form}` over the agent's absent fields"
        );
        assert!(
            selected(&ours[BARE_ASSIGN + 1]).is_some(),
            "`{form}` over the agent's written fields is decided: {:?}",
            ours[BARE_ASSIGN + 1]
        );
    }
}

#[test]
fn an_undecidable_guard_names_its_absent_field() {
    let ours = agreed("label == mine");
    for (step, refused, entity) in [
        (
            BARE_REVIEW,
            "desk.ticket.Review/held-refused",
            "desk.ticket.Ticket",
        ),
        (
            BARE_ASSIGN,
            "desk.ticket.Assign/agent-refused",
            "desk.ticket.Agent",
        ),
    ] {
        let Err(error) = &ours[step] else {
            panic!("the guard of `{refused}` is open: {:?}", ours[step])
        };
        assert_eq!(error.kind(), "undecidable", "{error}");
        assert_eq!(
            error.to_string(),
            format!(
                "the guard of `{refused}` is undecidable: `label == mine` is unknown on a \
                 `{entity}` row where `label` is absent; write `defined(label)` in the guard to rule \
                 such a row out"
            ),
        );
    }

    let ours = agreed("{all: [rank > 3, flagged]}");
    let Err(error) = &ours[BARE_REVIEW] else {
        panic!("both parts are open: {:?}", ours[BARE_REVIEW])
    };
    assert!(
        error.to_string().ends_with(
            "where `rank` or `flagged` is absent; write `defined(rank)` and `defined(flagged)` in \
             the guard to rule such a row out"
        ),
        "every absent field the guard reads is named: {error}"
    );
}
