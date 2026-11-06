//! An outcome that declares `returns: true` answers the command's declared `response:` fields with
//! values the runtime chooses, as ESS leaves them to the implementation; a field the outcome lists
//! in `one_time_response:` is originated fresh by each invocation.
//!
//! The fixture `tests/fixtures/responses` opens a notebook and shares it: `ShareNotebook` returns
//! `{link: String, share_code: String}` with `one_time_response: [share_code]`, records the
//! audience and publishes `NotebookShared`. Where the share code appears outside its own answer
//! (store, events, views, later answers) is asserted end to end on a served run
//! (`spindle-cli/tests/spindle-cli/responses.rs`).

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::direct_response::Observation;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Executed, Identity, MemoryStore, NoExternals, Uuid, execute, execute_minting,
};

const OPEN: &str = "notes.book.OpenNotebook";
const SHARE: &str = "notes.book.ShareNotebook";
const NOTEBOOK_ID: &str = "nb-1";

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/responses");
    spindle_spec::load(&root)
        .expect("the responses fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

fn open_input() -> Map<String, Value> {
    input(json!({"notebook_id": NOTEBOOK_ID, "title": "Field notes"}))
}

fn share_input(audience: &str) -> Map<String, Value> {
    input(json!({"notebook_id": NOTEBOOK_ID, "audience": audience}))
}

/// A mint that answers `00000000-0000-4000-8000-<n>` for its `n`th request and records each.
struct Counting {
    next: u64,
    asked: Vec<String>,
}

impl spindle_kernel::Mint for Counting {
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid {
        self.next += 1;
        self.asked.push(format!(
            "{} for {}/{}: {}.{}",
            identity.capability,
            identity.command,
            identity.outcome,
            identity.entity,
            identity.field
        ));
        Uuid::parse_str(&format!("00000000-0000-4000-8000-{:012}", self.next)).expect("a UUID")
    }
}

/// Opens the notebook, then shares it once per audience, each with a fresh random mint.
fn shares(audiences: &[&str]) -> Vec<Executed> {
    let ir = ir();
    let mut store = MemoryStore::default();
    execute(&ir, &mut store, &mut NoExternals, OPEN, &open_input()).expect("the notebook opens");
    audiences
        .iter()
        .map(|audience| {
            execute(
                &ir,
                &mut store,
                &mut NoExternals,
                SHARE,
                &share_input(audience),
            )
            .expect("sharing runs")
        })
        .collect()
}

fn response(executed: &Executed) -> &Map<String, Value> {
    executed
        .response
        .as_ref()
        .unwrap_or_else(|| panic!("`{}` answers a response: {executed:?}", executed.outcome))
}

fn share_code(executed: &Executed) -> String {
    response(executed)["share_code"]
        .as_str()
        .unwrap_or_else(|| panic!("the share code is text: {executed:?}"))
        .to_owned()
}

#[test]
fn a_returning_outcome_answers_its_typed_response() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut mint = Counting {
        next: 0,
        asked: Vec::new(),
    };
    execute(&ir, &mut store, &mut NoExternals, OPEN, &open_input()).expect("the notebook opens");
    let shared = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        SHARE,
        &share_input("readers"),
    )
    .expect("sharing runs");
    assert_eq!(shared.outcome, "shared");
    assert_eq!(shared.error, None);
    let answered = response(&shared);
    let command = ir
        .commands()
        .values()
        .find(|command| command.name.to_string() == SHARE)
        .expect("the fixture declares ShareNotebook");
    assert_eq!(
        answered.keys().cloned().collect::<Vec<_>>(),
        vec!["link".to_owned(), "share_code".to_owned()],
        "exactly the declared response fields: {answered:?}"
    );
    for field in &command.response {
        let value = &answered[&field.name];
        spindle_kernel::admits(&ir, &field.type_ref, value).unwrap_or_else(|error| {
            panic!(
                "`{}` answers a value of its declared type: {error}",
                field.name
            )
        });
    }
    assert_eq!(
        mint.asked,
        vec![
            "identity.mint for notes.book.ShareNotebook/shared: notes.book.ShareNotebook.link"
                .to_owned(),
            "identity.mint for notes.book.ShareNotebook/shared: notes.book.ShareNotebook.share_code"
                .to_owned(),
        ],
        "each response field is drawn once, through identity.mint"
    );
    assert_ne!(answered["link"], answered["share_code"]);

    let opened = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        OPEN,
        &input(json!({"notebook_id": "nb-2", "title": "Sketches"})),
    )
    .expect("a second notebook opens");
    assert_eq!(
        opened.response, None,
        "an outcome that does not return answers no response"
    );
    let unknown = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        SHARE,
        &input(json!({"notebook_id": "nb-none", "audience": "readers"})),
    )
    .expect("sharing an unknown notebook is answered");
    assert_eq!(unknown.outcome, "no-such-notebook");
    assert_eq!(unknown.response, None, "a refusal answers no response");
}

#[test]
fn a_second_invocation_originates_a_fresh_one_time_value() {
    let shared = shares(&["readers", "readers", "editors"]);
    let codes: Vec<String> = shared.iter().map(share_code).collect();
    for (index, code) in codes.iter().enumerate() {
        assert!(!code.is_empty(), "a one-time value is never empty");
        for (other, earlier) in codes[..index].iter().enumerate() {
            assert!(
                !code.contains(earlier.as_str()) && !earlier.contains(code.as_str()),
                "invocation {index} originated {code:?}, which invocation {other} disclosed as \
                 {earlier:?}"
            );
        }
        for (later, executed) in shared.iter().enumerate().skip(index + 1) {
            let text = Value::Object(response(executed).clone()).to_string();
            assert!(
                !text.contains(code.as_str()),
                "invocation {later} answered {text}, which holds invocation {index}'s one-time \
                 value {code:?}"
            );
        }
        let link = response(&shared[index])["link"]
            .as_str()
            .unwrap_or_default();
        assert!(
            !link.contains(code.as_str()),
            "the link of the same answer holds the one-time value: {link:?}"
        );
    }
}

fn node_map(response: &Map<String, Value>) -> BTreeMap<String, Node> {
    response
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                serde_json::from_value(value.clone()).expect("a JSON value is a node"),
            )
        })
        .collect()
}

#[test]
fn responses_agree_with_the_interpreter() {
    let ir = ir();
    let audiences = ["readers", "editors", "readers"];
    let kernel = shares(&audiences);

    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "notes.book/authored/responses"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("responses").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let run = |command: &str, input: &Map<String, Value>| {
        target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input: node_map(input),
                correlation: context.correlation.clone(),
            })
            .expect("the interpreter runs the command")
    };
    run(OPEN, &open_input());
    let command = ir
        .commands()
        .values()
        .find(|command| command.name.to_string() == SHARE)
        .expect("the fixture declares ShareNotebook");
    for (audience, executed) in audiences.iter().zip(&kernel) {
        let interpreted = run(SHARE, &share_input(audience));
        let selected = interpreted
            .outcome
            .clone()
            .expect("the interpreter selects an outcome");
        assert_eq!(selected.outcome.as_str(), executed.outcome);
        let interpreter_response = interpreted
            .response
            .as_ref()
            .expect("the interpreter answers a response for the same step");
        let check = Observation::of(&ir, command, Some(selected.clone()), BTreeMap::new())
            .expect("ESS derives the response check from the command");
        check
            .compare(Some(interpreter_response))
            .expect("the check accepts the interpreter's own response");
        let answered = node_map(response(executed));
        check.compare(Some(&answered)).unwrap_or_else(|refused| {
            panic!("ESS's response check refuses the kernel's response {answered:?}: {refused}")
        });
        let mut short = answered.clone();
        short.remove("share_code");
        assert!(
            check.compare(Some(&short)).is_err(),
            "the check refuses a response without its one-time field, so it checks something"
        );
    }
}

/// The response values the runtime chooses are drawn through `identity.mint`, so admission
/// derives it for the returning outcome, one reason per response field.
#[test]
fn a_returned_response_requires_identity_mint() {
    let ir = ir();
    let requirements = spindle_capability::derive(&ir, Some("desk")).expect("desk is a component");
    let minted = requirements
        .get(&spindle_capability::Capability::new(
            spindle_capability::Capability::IDENTITY_MINT,
        ))
        .expect("a returned response requires identity.mint");
    let because: Vec<(&str, &str)> = minted
        .iter()
        .map(|reason| (reason.name.as_str(), reason.because.as_str()))
        .collect();
    assert_eq!(
        because,
        [
            (
                "notes.book.ShareNotebook/shared",
                "mints the response field `notes.book.ShareNotebook.link`"
            ),
            (
                "notes.book.ShareNotebook/shared",
                "mints the response field `notes.book.ShareNotebook.share_code`"
            ),
        ]
    );
}
