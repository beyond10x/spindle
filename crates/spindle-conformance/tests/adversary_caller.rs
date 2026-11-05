//! Adversary cases for the caller a command is sent as (story:kernel-caller-attributes): the same
//! requests, actor and caller included, sent to ESS 0.86.0's interpreter and to the in-process
//! host, and the HTTP surface of the commands that read the caller.
//!
//! The fixture `tests/fixtures/adversary-caller` declares two actors. `Author` carries `owner_id`,
//! `code` (an `OwnerId`) and an optional `delegate`; `Visitor` carries `owner_id`, `code` (a
//! `String`) and `badge`. Both may send `Ping`, which reads nothing of the caller; only `Author`
//! sends the commands that read it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{ActorRef, CommandRef, ViewRef};
use ess_conformance::SemanticViewRequest;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::Deadline;
use ess_conformance::{ConformanceTarget, ScenarioContext, ScenarioId, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const SPEC: &str = "crates/spindle-conformance/tests/fixtures/adversary-caller";
const AUTHOR: &str = "quill.desk.Author";
const VISITOR: &str = "quill.desk.Visitor";
const PING: &str = "quill.desk.Ping";
const CLAIM: &str = "quill.desk.Claim";
const HAND: &str = "quill.desk.Hand";
const KEEP: &str = "quill.desk.Keep";
const CARD: &str = "quill.desk.DeskCard";

const DESK: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
const DESK_TWO: &str = "6c3e9a25-4d7b-4f8c-8e2a-3b4c5d6e7f80";
const OWNER: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const CODE: &str = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";
const DELEGATE: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";
const OTHER: &str = "2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join(SPEC))
        .expect("the adversary caller fixture compiles")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn text(value: &str) -> Node {
    Node::Text(value.to_owned())
}

/// One command step: the command, its input, the actor it is sent as and that actor's caller.
struct Step {
    command: &'static str,
    input: Vec<(&'static str, &'static str)>,
    actor: &'static str,
    caller: Vec<(&'static str, Node)>,
}

fn step(
    command: &'static str,
    input: &[(&'static str, &'static str)],
    actor: &'static str,
    caller: Vec<(&'static str, Node)>,
) -> Step {
    Step {
        command,
        input: input.to_vec(),
        actor,
        caller,
    }
}

/// What each step answered (`<outcome>` or `refused request`), and the `DeskCard` rows after
/// the last; the second list carries the reason of each refused request, for the message.
fn run(target: &impl ConformanceTarget, steps: &[Step]) -> (Vec<String>, Vec<String>, String) {
    let context = ScenarioContext::new(
        ScenarioId::parse("quill.desk.Ping/authored/adversary-caller").expect("a scenario id"),
        CorrelationId::new("adversary-caller").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    let mut reasons = Vec::new();
    for step in steps {
        let result = target.execute_command(SemanticCommandRequest {
            command: step.command.parse::<CommandRef>().expect("a command"),
            actor: Some(ActorRef::new(step.actor.parse().expect("an actor name"))),
            caller: Some(
                step.caller
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), value.clone()))
                    .collect(),
            ),
            input: step
                .input
                .iter()
                .map(|(field, value)| ((*field).to_owned(), text(value)))
                .collect(),
            correlation: context.correlation.clone(),
        });
        match result {
            Ok(result) => answers.push(
                result
                    .outcome
                    .map(|outcome| outcome.outcome.as_str().to_owned())
                    .unwrap_or_else(|| "<undeclared>".to_owned()),
            ),
            Err(error) => {
                answers.push("refused request".to_owned());
                reasons.push(format!("{}: {error:?}", step.command));
            }
        }
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(CARD.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the view reads")
        .rows;
    // An unset `Optional` field is published as `null` by ESS's interpreter and omitted by the
    // host; both read as absent here, so the rows compare what the caller decided.
    let mut rows: Vec<String> = rows
        .iter()
        .map(|row| {
            let set: BTreeMap<&String, &Node> = row
                .iter()
                .filter(|(_, value)| **value != Node::Null)
                .collect();
            format!("{set:?}")
        })
        .collect();
    rows.sort();
    let _ = target.end_scenario(&context);
    (answers, rows, reasons.join("\n"))
}

fn agree(steps: &[Step]) {
    let (ess, ess_rows, ess_why) = run(&Interpreted::for_model(ir()), steps);
    let (spindle, spindle_rows, spindle_why) = run(&HostTarget::new(ir(), laptop()), steps);
    assert_eq!(
        spindle, ess,
        "the host answers each step as ESS's interpreter does\n  ESS refused:\n{ess_why}\n  \
         spindle refused:\n{spindle_why}"
    );
    assert_eq!(spindle_rows, ess_rows, "the rows agree");
}

/// ESS binds a caller against the attributes of the actor it is sent as with
/// `Completeness::Total` (`interpret/execute/caller.rs` `Caller::bind`): a caller that leaves a
/// required attribute out is not a caller of that actor, and the request is refused before the
/// command runs, whether or not the command reads that attribute. Spindle checks only what the
/// caller carries, so an `Author` without its required `code` opens a desk.
#[test]
fn a_caller_without_a_required_attribute_of_its_actor_is_refused_as_ess_refuses_it() {
    agree(&[step(
        PING,
        &[("desk_id", DESK), ("label", "hall")],
        AUTHOR,
        vec![("owner_id", text(OWNER))],
    )]);
}

/// ESS binds the caller against the sending actor's own attributes. Spindle checks it against
/// the union of every actor that may send the command, typed by the first declaration it meets,
/// so a `Visitor`, whose `code` is a `String`, is refused because `Author` declares `code` as an
/// `OwnerId`.
#[test]
fn a_visitor_caller_is_typed_by_the_visitor_s_attributes() {
    agree(&[step(
        PING,
        &[("desk_id", DESK), ("label", "hall")],
        VISITOR,
        vec![
            ("owner_id", text(OWNER)),
            ("code", text("v-1")),
            ("badge", text("b-7")),
        ],
    )]);
}

/// An `Author` caller carrying `badge`, which only `Visitor` declares, is not a caller of
/// `Author`: ESS refuses the request (`UndeclaredField`); Spindle accepts it because some actor
/// that may send `Ping` declares `badge`.
#[test]
fn an_attribute_only_another_actor_declares_is_refused() {
    agree(&[step(
        PING,
        &[("desk_id", DESK), ("label", "hall")],
        AUTHOR,
        vec![
            ("owner_id", text(OWNER)),
            ("code", text(CODE)),
            ("badge", text("b-7")),
        ],
    )]);
}

/// An `Optional` caller attribute, absent, `null` and present, as a value source and as a guard
/// operand against an input field and against a stored `Optional` field.
#[test]
fn an_optional_caller_attribute_agrees_with_the_interpreter() {
    let bare = || vec![("owner_id", text(OWNER)), ("code", text(CODE))];
    let with = |delegate: &str| {
        vec![
            ("owner_id", text(OWNER)),
            ("code", text(CODE)),
            ("delegate", text(delegate)),
        ]
    };
    let null = || {
        vec![
            ("owner_id", text(OWNER)),
            ("code", text(CODE)),
            ("delegate", Node::Null),
        ]
    };
    agree(&[
        step(
            CLAIM,
            &[("desk_id", DESK), ("label", "hall")],
            AUTHOR,
            bare(),
        ),
        step(HAND, &[("desk_id", DESK), ("to", OTHER)], AUTHOR, bare()),
        step(
            KEEP,
            &[("desk_id", DESK), ("label", "attic")],
            AUTHOR,
            bare(),
        ),
        step(
            KEEP,
            &[("desk_id", DESK), ("label", "loft")],
            AUTHOR,
            null(),
        ),
        step(
            KEEP,
            &[("desk_id", DESK), ("label", "cellar")],
            AUTHOR,
            with(DELEGATE),
        ),
        step(
            CLAIM,
            &[("desk_id", DESK_TWO), ("label", "porch")],
            AUTHOR,
            with(DELEGATE),
        ),
        step(
            KEEP,
            &[("desk_id", DESK_TWO), ("label", "deck")],
            AUTHOR,
            bare(),
        ),
        step(
            HAND,
            &[("desk_id", DESK_TWO), ("to", DELEGATE)],
            AUTHOR,
            with(DELEGATE),
        ),
        step(
            KEEP,
            &[("desk_id", DESK_TWO), ("label", "roof")],
            AUTHOR,
            with(DELEGATE),
        ),
    ]);
}
