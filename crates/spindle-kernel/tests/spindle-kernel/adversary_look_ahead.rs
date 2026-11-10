//! Adversary pass 2 for the look-ahead beside a missing related row (`select_ahead`,
//! `ahead_answer`, `missing_reference`), each request run against ESS 0.90's interpreter, which is
//! the authority.
//!
//! The fixture `tests/fixtures/adversary-look-ahead` declares desks that open and close, and
//! books shelved at a desk (the `home` they store) and lent from one. Every probe is run on a
//! fresh store after the same setup, once with the decider declining every external branch and
//! once for each external branch it takes, and the kernel must answer what the interpreter
//! answers under the same forcing. Where the interpreter answers the same under every forcing, the
//! model decides the request alone, and the kernel must answer it with a decider that has no
//! answer at all.

use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, OutcomeRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, ExternalOutcomeControl, ScenarioContext, SemanticCommandRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{External, MemoryStore, execute};

const OPEN: &str = "probe.ahead.OpenDesk";
const CLOSE: &str = "probe.ahead.CloseDesk";
const SHELVE: &str = "probe.ahead.ShelveBook";
const LEND_PLAIN: &str = "probe.ahead.LendPlain";
const LEND_QUIET: &str = "probe.ahead.LendQuiet";
const RETURN_QUIET: &str = "probe.ahead.ReturnQuiet";
const LEND_APPROVED: &str = "probe.ahead.LendApproved";
const LEND_TWO: &str = "probe.ahead.LendTwo";

const OAK: &str = "5d6e7f80-0000-4000-8000-000000000001";
const ELM: &str = "5d6e7f80-0000-4000-8000-000000000002";
const NOWHERE: &str = "5d6e7f80-0000-4000-8000-0000000000ff";

fn book(n: u8) -> String {
    format!("9a8b7c6d-0000-4000-8000-0000000000{n:02x}")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-look-ahead")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the adversary-look-ahead fixture compiles")
        .ir
}

/// One request: the command, its input, and the `external:` outcome the decider takes, if any.
type Step = (&'static str, Value, Option<&'static str>);

/// What one request answered: the outcome, the declared error and the events by name, or that
/// no answer was given.
type Answer = Result<(String, Option<String>, Vec<String>), ()>;

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

/// Two desks, `ELM` closed. Book 1 shelved at `OAK`, 2 lent from it; 3 shelved at a desk no row
/// carries, 4 lent while homed there; 5 lent while homed at the closed desk.
fn setup() -> Vec<Step> {
    vec![
        (OPEN, json!({"desk_id": OAK, "label": "oak"}), None),
        (OPEN, json!({"desk_id": ELM, "label": "elm"}), None),
        (SHELVE, json!({"book_id": book(1), "home": OAK}), None),
        (SHELVE, json!({"book_id": book(2), "home": OAK}), None),
        (LEND_PLAIN, json!({"book_id": book(2)}), None),
        (SHELVE, json!({"book_id": book(3), "home": NOWHERE}), None),
        (SHELVE, json!({"book_id": book(4), "home": NOWHERE}), None),
        (LEND_PLAIN, json!({"book_id": book(4)}), None),
        (SHELVE, json!({"book_id": book(5), "home": ELM}), None),
        (LEND_PLAIN, json!({"book_id": book(5)}), None),
        (CLOSE, json!({"desk_id": ELM}), None),
    ]
}

/// The interpreter's answer to the last of `steps`, each forced as it names.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> Answer {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.ahead/authored/adversary-look-ahead"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-look-ahead").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut last = Err(());
    for (command, input, forced) in steps {
        if let Some(outcome) = forced {
            target
                .configure_external_outcome(ExternalOutcomeControl {
                    force: OutcomeRef::new(
                        command.parse::<CommandRef>().expect("a command"),
                        OutcomeName::new(*outcome).expect("an outcome name"),
                    ),
                    correlation: context.correlation.clone(),
                })
                .expect("the interpreter takes the forcing");
        }
        let input = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let executed = target.execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input,
            correlation: context.correlation.clone(),
        });
        last = executed.map_err(|_| ()).map(|executed| {
            (
                executed
                    .outcome
                    .map(|outcome| outcome.outcome.as_str().to_owned())
                    .unwrap_or_default(),
                executed.error.map(|error| error.error.to_string()),
                executed
                    .direct_events
                    .iter()
                    .map(|event| event.event.to_string())
                    .collect(),
            )
        });
    }
    last
}

/// What the kernel answers to the last of `steps`, and the external branches it asked about for
/// it. `answers: false` gives the last request a decider with no answer.
fn spindled(ir: &spindle_spec::EssIr, steps: &[Step], answers: bool) -> (Answer, Vec<String>) {
    let mut store = MemoryStore::default();
    let mut last = Err(());
    let mut asked = Vec::new();
    for (index, (command, input, forced)) in steps.iter().enumerate() {
        let probe = index + 1 == steps.len();
        asked.clear();
        let mut decider = |question: &External<'_>| {
            asked.push(question.outcome.to_owned());
            if probe && !answers {
                return Err(format!("no decision for {}", question.capability));
            }
            Ok(Some(question.outcome) == *forced)
        };
        let executed = execute(
            ir,
            &mut store,
            &mut decider,
            command,
            &object(input.clone()),
        );
        if !probe {
            executed
                .as_ref()
                .unwrap_or_else(|error| panic!("setup step {command} {input} answers: {error}"));
        }
        last = executed.map_err(|_| ()).map(|executed| {
            (
                executed.outcome,
                executed.error.map(|error| error.name),
                executed
                    .events
                    .into_iter()
                    .map(|event| event.name)
                    .collect(),
            )
        });
    }
    (last, asked)
}

fn outcome(answer: &Answer) -> String {
    match answer {
        Ok((outcome, _, _)) => outcome.clone(),
        Err(()) => "<no answer>".to_owned(),
    }
}

fn with_setup(probe: &Step) -> Vec<Step> {
    let mut steps = setup();
    steps.push(probe.clone());
    steps
}

/// Each probe, unforced and forced to each of `externals`: the kernel answers what the
/// interpreter does.
fn agree(ir: &spindle_spec::EssIr, probes: &[(&'static str, Value)], externals: &[&'static str]) {
    let mut differing = Vec::new();
    for (command, input) in probes {
        for forced in std::iter::once(None).chain(externals.iter().copied().map(Some)) {
            let steps = with_setup(&(*command, input.clone(), forced));
            let ess = interpreted(ir, &steps);
            let (kernel, asked) = spindled(ir, &steps, true);
            if ess != kernel {
                differing.push(format!(
                    "{command} {input} forced {forced:?}: interpreter `{}`, kernel `{}` (asked {asked:?})",
                    outcome(&ess),
                    outcome(&kernel)
                ));
            }
        }
    }
    assert!(
        differing.is_empty(),
        "the kernel answers differently from the interpreter:\n  {}",
        differing.join("\n  ")
    );
}

/// Each probe the interpreter answers the same under every forcing is the model's alone: the
/// kernel answers it with a decider that has no answer.
fn decided_by_the_model(
    ir: &spindle_spec::EssIr,
    probes: &[(&'static str, Value)],
    externals: &[&'static str],
) {
    let mut differing = Vec::new();
    for (command, input) in probes {
        let answers: Vec<Answer> = std::iter::once(None)
            .chain(externals.iter().copied().map(Some))
            .map(|forced| interpreted(ir, &with_setup(&(*command, input.clone(), forced))))
            .collect();
        let ess = &answers[0];
        if ess.is_err() || answers.iter().any(|answer| answer != ess) {
            continue;
        }
        let (kernel, asked) = spindled(ir, &with_setup(&(*command, input.clone(), None)), false);
        if &kernel != ess {
            differing.push(format!(
                "{command} {input}: interpreter `{}` under every forcing, kernel `{}` (asked {asked:?})",
                outcome(ess),
                outcome(&kernel)
            ));
        }
    }
    assert!(
        differing.is_empty(),
        "the kernel asks a decider for a request the model answers alone:\n  {}",
        differing.join("\n  ")
    );
}

fn lend_quiet_probes() -> Vec<(&'static str, Value)> {
    let mut probes = Vec::new();
    for n in [1, 2] {
        for desk in [NOWHERE, ELM, OAK] {
            probes.push((LEND_QUIET, json!({"book_id": book(n), "desk_id": desk})));
            probes.push((
                LEND_QUIET,
                json!({"book_id": book(n), "desk_id": desk, "quiet": true}),
            ));
            probes.push((
                LEND_QUIET,
                json!({"book_id": book(n), "desk_id": desk, "quiet": false}),
            ));
        }
    }
    probes
}

fn return_quiet_probes() -> Vec<(&'static str, Value)> {
    let mut probes = Vec::new();
    for n in [1, 2, 3, 4, 5, 0x99] {
        probes.push((RETURN_QUIET, json!({"book_id": book(n)})));
        probes.push((RETURN_QUIET, json!({"book_id": book(n), "quiet": true})));
    }
    probes
}

fn lend_approved_probes() -> Vec<(&'static str, Value)> {
    let mut probes = Vec::new();
    for n in [1, 2] {
        for desk in [NOWHERE, ELM, OAK] {
            probes.push((LEND_APPROVED, json!({"book_id": book(n), "desk_id": desk})));
        }
    }
    probes
}

fn lend_two_probes() -> Vec<(&'static str, Value)> {
    let mut probes = Vec::new();
    for n in [1, 2] {
        for desk in [NOWHERE, ELM, OAK] {
            for spare in [NOWHERE, ELM, OAK] {
                probes.push((
                    LEND_TWO,
                    json!({"book_id": book(n), "desk_id": desk, "spare_id": spare}),
                ));
            }
        }
    }
    probes
}

/// An external refusal declared before an accepting branch whose input guard reads an Optional
/// input the request leaves out, beside a desk no row carries. Taken, the interpreter's look-ahead
/// stops at the refusal and the missing desk answers `no-desk`; the guard after it is never read.
#[test]
fn adversary_a_taken_refusal_before_an_unknown_guard_beside_a_missing_input_row_agrees() {
    agree(&ir(), &lend_quiet_probes(), &["withheld"]);
}

/// The same over a stored reference (`stored_reference`'s look-ahead): a book whose home no desk
/// carries, returned while the reading room withholds it.
#[test]
fn adversary_a_taken_refusal_before_an_unknown_guard_beside_a_missing_stored_row_agrees() {
    agree(&ir(), &return_quiet_probes(), &["withheld"]);
}

/// An accepting `external:` move beside the same move unguarded, with a missing, a closed and an
/// open desk.
#[test]
fn adversary_an_external_move_beside_a_missing_input_row_agrees() {
    agree(&ir(), &lend_approved_probes(), &["approved"]);
}

/// Two rows read through the input, each missing, closed or open.
#[test]
fn adversary_two_input_rows_one_missing_agree() {
    agree(&ir(), &lend_two_probes(), &["withheld"]);
}

/// No decider is asked for a request the model answers alone, on every command of the fixture.
#[test]
#[ignore = "pre-existing: story:look-ahead-asks-only-where-it-matters"]
fn adversary_no_decider_is_asked_for_a_request_the_model_answers_alone() {
    let ir = ir();
    decided_by_the_model(&ir, &lend_quiet_probes(), &["withheld"]);
    decided_by_the_model(&ir, &return_quiet_probes(), &["withheld"]);
    decided_by_the_model(&ir, &lend_two_probes(), &["withheld"]);
    decided_by_the_model(&ir, &lend_approved_probes(), &["approved"]);
}

/// The same, one command each, so a red one does not hide the next.
#[test]
#[ignore = "pre-existing: story:look-ahead-asks-only-where-it-matters"]
fn adversary_return_quiet_asks_no_decider_for_a_request_the_model_answers_alone() {
    decided_by_the_model(&ir(), &return_quiet_probes(), &["withheld"]);
}

#[test]
#[ignore = "pre-existing: story:look-ahead-asks-only-where-it-matters"]
fn adversary_lend_two_asks_no_decider_for_a_request_the_model_answers_alone() {
    decided_by_the_model(&ir(), &lend_two_probes(), &["withheld"]);
}

#[test]
#[ignore = "pre-existing: story:look-ahead-asks-only-where-it-matters"]
fn adversary_lend_approved_asks_no_decider_for_a_request_the_model_answers_alone() {
    decided_by_the_model(&ir(), &lend_approved_probes(), &["approved"]);
}

const FAST: &str = "5d6e7f80-0000-4000-8000-000000000003";
const LEND_MIXED: &str = "probe.ahead.LendMixed";

/// The branches of `LendMixed` whose declaration order is permuted: an `exists: false` row, a
/// present-related refusal, an external branch narrowed by `when_subject:`, an `external:`
/// refusal and an accepting `when_related:` move.
const MIXED: [&str; 5] = [
    "      - name: no-desk
        when_related:
          via: input.desk_id
          exists: false
        error: probe.ahead.NoDesk
        summary: The named desk does not exist.
",
    "      - name: desk-closed
        when_related:
          via: input.desk_id
          predicate: state == Closed
        error: probe.ahead.DeskShut
        summary: The named desk is closed.
",
    "      - name: held-back
        external: the reading room holds a shelved book back
        when_subject: {predicate: state == Shelved}
        error: probe.ahead.Withheld
        summary: The reading room holds the shelved book back.
",
    "      - name: withheld
        external: the reading room withholds the book
        error: probe.ahead.Withheld
        summary: The reading room withholds the book.
",
    "      - name: fast
        when_related:
          via: input.desk_id
          predicate: label == \"fast\"
        moves: probe.ahead.Book.lend
        instance: book_id
        emits: [probe.ahead.BookLentQuietly]
        payload:
          probe.ahead.BookLentQuietly:
            book_id: input.book_id
        summary: The book is lent quietly from a fast desk.
",
];

const MIXED_TAIL: &str = "      - name: lent
        moves: probe.ahead.Book.lend
        instance: book_id
        emits: [probe.ahead.BookLent]
        payload:
          probe.ahead.BookLent:
            book_id: input.book_id
        summary: The book is lent from the named desk.
      - name: wrong-state
        wrong_state: true
        error: probe.ahead.NotLent
        summary: The book is already lent.
";

/// Every permutation of `0..n`, in lexicographic order.
fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for rest in permutations(n - 1) {
        for at in 0..=rest.len() {
            let mut order = rest.clone();
            order.insert(at, n - 1);
            out.push(order);
        }
    }
    out.sort();
    out
}

/// The fixture with `LendMixed` added, its branches declared in `order`; `None` where ESS refuses
/// that order.
fn mixed(order: &[usize]) -> Option<(tempfile::TempDir, spindle_spec::EssIr)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    let system = std::fs::read_to_string(fixture().join("system.yaml"))
        .expect("the system reads")
        .replacen(
            "        - probe.ahead.LendTwo\n",
            "        - probe.ahead.LendTwo\n        - probe.ahead.LendMixed\n",
            1,
        );
    let mut command = String::from(
        "  - name: probe.ahead.LendMixed
    input:
      - name: book_id
        type: probe.ahead.BookId
      - name: desk_id
        type: probe.ahead.DeskId
    outcomes:
",
    );
    for index in order {
        command.push_str(MIXED[*index]);
    }
    command.push_str(MIXED_TAIL);
    let domain = std::fs::read_to_string(fixture().join("domains/ahead.yaml"))
        .expect("the domain reads")
        .replacen("\nevents:\n", &format!("\n{command}events:\n"), 1);
    std::fs::write(dir.path().join("system.yaml"), system).expect("the system writes");
    std::fs::write(dir.path().join("domains/ahead.yaml"), domain).expect("the domain writes");
    let ir = spindle_spec::load(dir.path()).ok()?.ir;
    Some((dir, ir))
}

/// Every declaration order of the five branches of `LendMixed`, each request unforced and forced
/// to each external branch: books shelved and lent, at a missing, a closed, an open and a fast
/// desk. The kernel answers what the interpreter does, and asks no decider for a request the
/// interpreter answers the same under every forcing.
#[test]
fn adversary_every_order_of_exists_false_when_subject_when_related_and_external_agrees() {
    let externals = ["withheld", "held-back"];
    let mut compiled = 0;
    let mut differing = Vec::new();
    for order in permutations(MIXED.len()) {
        let Some((_dir, ir)) = mixed(&order) else {
            continue;
        };
        compiled += 1;
        for n in [1, 2] {
            for desk in [NOWHERE, ELM, OAK, FAST] {
                let input = json!({"book_id": book(n), "desk_id": desk});
                let steps = |forced: Option<&'static str>| {
                    let mut steps = setup();
                    steps.push((OPEN, json!({"desk_id": FAST, "label": "fast"}), None));
                    steps.push((LEND_MIXED, input.clone(), forced));
                    steps
                };
                let mut answers = Vec::new();
                for forced in std::iter::once(None).chain(externals.iter().copied().map(Some)) {
                    let ess = interpreted(&ir, &steps(forced));
                    let (kernel, asked) = spindled(&ir, &steps(forced), true);
                    if ess != kernel {
                        differing.push(format!(
                            "order {order:?}, {input} forced {forced:?}: interpreter `{}`, kernel `{}` (asked {asked:?})",
                            outcome(&ess),
                            outcome(&kernel)
                        ));
                    }
                    answers.push(ess);
                }
                if answers[0].is_ok() && answers.iter().all(|answer| *answer == answers[0]) {
                    let (kernel, asked) = spindled(&ir, &steps(None), false);
                    if kernel != answers[0] {
                        differing.push(format!(
                            "order {order:?}, {input}: interpreter `{}` under every forcing, kernel `{}` with no decision (asked {asked:?})",
                            outcome(&answers[0]),
                            outcome(&kernel)
                        ));
                    }
                }
            }
        }
    }
    eprintln!("{compiled} declaration orders of LendMixed compiled");
    assert!(compiled > 0, "no declaration order of LendMixed compiles");
    assert!(
        differing.is_empty(),
        "{compiled} orders compiled; the kernel answers differently from the interpreter:\n  {}",
        differing.join("\n  ")
    );
}
