//! Who may send what (story:actor-command-grants), held to ESS 0.87.0's interpreter: the same
//! requests, each sent as an actor or as none, answer the same on the in-process host, and the
//! `…/grant/denied` scenarios ESS synthesises for a served component pass on it.
//!
//! The fixture `crates/spindle-host/tests/fixtures/grants` declares `Reader`, whose `may:` lists
//! only `ReadBook`, and `Librarian`, who may add, read and delete a book.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{ActorRef, CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::Deadline;
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, ScenarioContext, ScenarioId, SemanticCommandRequest,
    SemanticViewRequest, Status,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const SPEC: &str = "crates/spindle-host/tests/fixtures/grants";
const ADD: &str = "shelf.books.AddBook";
const READ: &str = "shelf.books.ReadBook";
const DELETE: &str = "shelf.books.DeleteBook";
const READER: &str = "shelf.books.Reader";
const LIBRARIAN: &str = "shelf.books.Librarian";
const BOOKS: &str = "shelf.books.Books";
const BOOK: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join(SPEC))
        .expect("the grants fixture compiles")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// One send: the command, its input, and the actor it is sent as, or none.
type Send = (
    &'static str,
    &'static [(&'static str, &'static str)],
    Option<&'static str>,
);

/// What each send answered, with the refusal's error where it was refused, and the view's rows.
fn run(target: &impl ConformanceTarget, sends: &[Send]) -> (Vec<String>, Vec<String>) {
    let context = ScenarioContext::new(
        ScenarioId::parse("shelf.books.ReadBook/authored/grants").expect("a scenario id"),
        CorrelationId::new("grants").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input, actor) in sends {
        let result = target.execute_command(SemanticCommandRequest {
            command: command.parse::<CommandRef>().expect("a command"),
            actor: actor.map(|actor| ActorRef::new(actor.parse().expect("an actor name"))),
            caller: None,
            input: input
                .iter()
                .map(|(field, value)| ((*field).to_owned(), Node::Text((*value).to_owned())))
                .collect(),
            correlation: context.correlation.clone(),
        });
        answers.push(match result {
            Ok(result) => format!(
                "{command}: {}{}",
                result.outcome.map_or_else(
                    || "<undeclared>".to_owned(),
                    |o| o.outcome.as_str().to_owned()
                ),
                result
                    .error
                    .map(|error| format!(" with {}", error.error))
                    .unwrap_or_default()
            ),
            Err(error) => format!("{command}: {error}"),
        });
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(BOOKS.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the view reads")
        .rows;
    // An unset `Optional` field is published as `null` by ESS's interpreter and omitted by the
    // host; both read as absent here.
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
    (answers, rows)
}

#[test]
fn command_grants_agree_with_the_interpreter() {
    let add: &[(&str, &str)] = &[("book_id", BOOK), ("title", "field notes")];
    let read: &[(&str, &str)] = &[("book_id", BOOK), ("note", "worn spine")];
    let delete: &[(&str, &str)] = &[("book_id", BOOK)];
    let sends: &[Send] = &[
        (ADD, add, Some(READER)),
        (ADD, add, Some(LIBRARIAN)),
        (DELETE, delete, Some(READER)),
        (READ, read, Some(READER)),
        (READ, read, Some("shelf.books.Visitor")),
        (DELETE, delete, None),
        (DELETE, delete, Some(LIBRARIAN)),
    ];
    let (ess, ess_rows) = run(&Interpreted::for_model(ir()), sends);
    let (spindle, spindle_rows) = run(&HostTarget::new(ir(), laptop()), sends);
    assert_eq!(
        spindle, ess,
        "the host answers each send as ESS's interpreter does"
    );
    assert_eq!(spindle_rows, ess_rows, "the rows agree");
    assert_eq!(
        ess[2],
        format!("{DELETE}: refused as not granted to `{READER}`"),
        "the reader's delete is refused as not granted: {ess:?}"
    );
}

#[test]
fn the_synthesised_grant_denied_scenarios_pass_on_the_host() {
    let ir = ir();
    let synthesis = spindle_conformance::synthesize(&ir);
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let statuses = |run: &spindle_conformance::Run| {
        run.report()
            .scenarios
            .iter()
            .map(|result| (result.scenario.to_string(), result.status))
            .collect::<BTreeMap<_, _>>()
    };
    let ess = statuses(&spindle_conformance::execute(
        &suite,
        &Interpreted::for_model(ir.clone()),
    ));
    let spindle = statuses(&spindle_conformance::execute(
        &suite,
        &HostTarget::new(ir, laptop()),
    ));
    let denied = ess
        .keys()
        .filter(|scenario| scenario.ends_with("/grant/denied"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        denied,
        vec![
            format!("{ADD}/grant/denied"),
            format!("{DELETE}/grant/denied")
        ],
        "ESS synthesises a refusal for each command the reader lacks: {ess:?}"
    );
    for scenario in &denied {
        assert_eq!(
            ess.get(scenario),
            Some(&Status::Passed),
            "the interpreter: {ess:?}"
        );
        assert_eq!(
            spindle.get(scenario),
            Some(&Status::Passed),
            "the host: {spindle:?}"
        );
    }
    let divergent = spindle
        .iter()
        .filter(|(_, status)| **status != Status::Unsupported)
        .filter(|(scenario, status)| ess.get(*scenario) != Some(status))
        .collect::<Vec<_>>();
    assert!(
        divergent.is_empty(),
        "ess: {ess:?}\nspindle: {spindle:?}\ndivergent: {divergent:?}"
    );
}
