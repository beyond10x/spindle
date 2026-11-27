//! Adversary pass for wave 30 U4 (story:look-ahead-asks-only-where-it-matters): the look-ahead
//! now runs the walk that asks no decider first, on every command, and answers without asking
//! where every branch the asking walk could select gives the same answer.
//!
//! Each command below is added to the fixture `tests/fixtures/adversary-look-ahead` in a scratch
//! copy. The permutation cases run every declaration order of a command's branches against ESS's
//! interpreter, unforced and forced to each external branch, and check that a request the
//! interpreter answers the same under every forcing is answered with a decider that has no answer:
//! an external move beside an external refusal (two external branches whose look-ahead answers
//! differ), and the same over a stored reference. The last cases put a row-set guard after an
//! external move: the walk that asks no decider reads past the external branch, so what it reads
//! there is read even where the decider takes that branch.

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
use spindle_kernel::{
    Change, ExecuteError, External, MemoryStore, NoExternals, Removal, Scan, Store, StoreError,
    Stored, Write, execute,
};

const OPEN: &str = "probe.ahead.OpenDesk";
const CLOSE: &str = "probe.ahead.CloseDesk";
const SHELVE: &str = "probe.ahead.ShelveBook";
const LEND_PLAIN: &str = "probe.ahead.LendPlain";
const LEND_BOTH: &str = "probe.ahead.LendBoth";
const RETURN_BOTH: &str = "probe.ahead.ReturnBoth";
const LEND_BUSY: &str = "probe.ahead.LendBusy";
const DESK: &str = "probe.ahead.Desk";

const OAK: &str = "5d6e7f80-0000-4000-8000-000000000001";
const ELM: &str = "5d6e7f80-0000-4000-8000-000000000002";
const FIR: &str = "5d6e7f80-0000-4000-8000-000000000004";
const NOWHERE: &str = "5d6e7f80-0000-4000-8000-0000000000ff";

fn book(n: u8) -> String {
    format!("9a8b7c6d-0000-4000-8000-0000000000{n:02x}")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-look-ahead")
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
/// carries, 4 lent while homed there; 5 lent while homed at the closed desk; 6 lent with no home.
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
        (SHELVE, json!({"book_id": book(6)}), None),
        (LEND_PLAIN, json!({"book_id": book(6)}), None),
        (CLOSE, json!({"desk_id": ELM}), None),
    ]
}

/// The fixture with `command` (its YAML, named `name`) added; `None` where ESS refuses it.
fn with_command(name: &str, command: &str) -> Option<(tempfile::TempDir, spindle_spec::EssIr)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    let system = std::fs::read_to_string(fixture().join("system.yaml"))
        .expect("the system reads")
        .replacen(
            "        - probe.ahead.LendTwo\n",
            &format!("        - probe.ahead.LendTwo\n        - {name}\n"),
            1,
        );
    let domain = std::fs::read_to_string(fixture().join("domains/ahead.yaml"))
        .expect("the domain reads")
        .replacen("\nevents:\n", &format!("\n{command}events:\n"), 1);
    std::fs::write(dir.path().join("system.yaml"), system).expect("the system writes");
    std::fs::write(dir.path().join("domains/ahead.yaml"), domain).expect("the domain writes");
    let ir = spindle_spec::load(dir.path()).ok()?.ir;
    Some((dir, ir))
}

/// The interpreter's answer to the last of `steps`, each forced as it names.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> Answer {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.ahead/authored/adversary-w30-u4"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w30-u4").expect("a correlation id"),
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

/// Each request of `inputs`, after `setup`, on `command` built from `head`, the `pieces` in every
/// declaration order and `tail`: the kernel answers what the interpreter does under each forcing,
/// and answers a request the interpreter answers the same under every forcing with no decision.
#[allow(clippy::too_many_arguments)]
fn every_order_agrees(
    name: &'static str,
    header: &str,
    head: &str,
    pieces: &[&str],
    tail: &str,
    inputs: &[Value],
    externals: &[&'static str],
) {
    let mut compiled = 0;
    let mut differing = Vec::new();
    for order in permutations(pieces.len()) {
        let mut command = String::from(header);
        command.push_str(head);
        for index in &order {
            command.push_str(pieces[*index]);
        }
        command.push_str(tail);
        let Some((_dir, ir)) = with_command(name, &command) else {
            continue;
        };
        compiled += 1;
        for input in inputs {
            let steps = |forced: Option<&'static str>| {
                let mut steps = setup();
                steps.push((name, input.clone(), forced));
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
    eprintln!("{compiled} declaration orders of {name} compiled");
    assert!(compiled > 0, "no declaration order of {name} compiles");
    assert!(
        differing.is_empty(),
        "{compiled} orders compiled; the kernel answers differently from the interpreter:\n  {}",
        differing.join("\n  ")
    );
}

const LEND_BOTH_HEADER: &str = "  - name: probe.ahead.LendBoth
    input:
      - name: book_id
        type: probe.ahead.BookId
      - name: desk_id
        type: probe.ahead.DeskId
      - name: quiet
        type: Optional<Boolean>
    outcomes:
";

/// An `exists: false` row, a present-related refusal, an external refusal, an external move and
/// an input-guarded move over an `Optional` input.
const LEND_BOTH_PIECES: [&str; 5] = [
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
    "      - name: withheld
        external: the reading room withholds the book
        error: probe.ahead.Withheld
        summary: The reading room withholds the book.
",
    "      - name: approved
        external: the reading room approves a quiet loan
        moves: probe.ahead.Book.lend
        instance: book_id
        emits: [probe.ahead.BookLentQuietly]
        payload:
          probe.ahead.BookLentQuietly:
            book_id: input.book_id
        summary: The book is lent quietly, as the reading room approved.
",
    "      - name: lent-quietly
        when: quiet == true
        moves: probe.ahead.Book.lend
        instance: book_id
        emits: [probe.ahead.BookLentQuietly]
        payload:
          probe.ahead.BookLentQuietly:
            book_id: input.book_id
        summary: The book is lent without a notice.
",
];

const LEND_TAIL: &str = "      - name: lent
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

/// Two external branches whose look-ahead answers differ for a lent book (a refusal acts on no
/// row, a move answers the wrong state), every declaration order, with a missing, a closed and an
/// open desk, the `Optional` input left out and given.
#[test]
fn adversary_w30_u4_every_order_of_an_external_move_beside_an_external_refusal_agrees() {
    let mut inputs = Vec::new();
    for n in [1, 2] {
        for desk in [NOWHERE, ELM, OAK] {
            inputs.push(json!({"book_id": book(n), "desk_id": desk}));
            inputs.push(json!({"book_id": book(n), "desk_id": desk, "quiet": true}));
        }
    }
    every_order_agrees(
        LEND_BOTH,
        LEND_BOTH_HEADER,
        "",
        &LEND_BOTH_PIECES,
        LEND_TAIL,
        &inputs,
        &["withheld", "approved"],
    );
}

const RETURN_BOTH_HEADER: &str = "  - name: probe.ahead.ReturnBoth
    input:
      - name: book_id
        type: probe.ahead.BookId
    outcomes:
";

const RETURN_BOTH_HEAD: &str = "      - name: no-book
        unknown_instance: true
        error: probe.ahead.NoBook
        summary: No book carries the id.
";

/// A stored `exists: false` row, a present-related refusal over it, an external refusal, an
/// external move and an external refusal narrowed by `when_subject:`.
const RETURN_BOTH_PIECES: [&str; 5] = [
    "      - name: home-gone
        when_related:
          via: home
          exists: false
        error: probe.ahead.NoDesk
        summary: The book's own desk does not exist.
",
    "      - name: home-closed
        when_related:
          via: home
          predicate: state == Closed
        error: probe.ahead.DeskShut
        summary: The book's own desk is closed.
",
    "      - name: withheld
        external: the reading room withholds the book
        error: probe.ahead.Withheld
        summary: The reading room withholds the book.
",
    "      - name: approved
        external: the reading room approves a quiet return
        moves: probe.ahead.Book.back
        instance: book_id
        emits: [probe.ahead.BookReturnedQuietly]
        payload:
          probe.ahead.BookReturnedQuietly:
            book_id: input.book_id
        summary: The book goes back quietly, as the reading room approved.
",
    "      - name: held-back
        external: the reading room holds a lent book back
        when_subject: {predicate: state == Lent}
        error: probe.ahead.Withheld
        summary: The reading room holds the lent book back.
",
];

const RETURN_TAIL: &str = "      - name: returned
        moves: probe.ahead.Book.back
        instance: book_id
        emits: [probe.ahead.BookReturned]
        payload:
          probe.ahead.BookReturned:
            book_id: input.book_id
        summary: The book goes back to its own desk.
      - name: wrong-state
        wrong_state: true
        error: probe.ahead.NotLent
        summary: The book is not lent.
";

/// The same over a stored reference (`stored_reference`'s look-ahead, which reads no held-state
/// guard): every declaration order, books shelved and lent, homed at an open, a closed and a
/// missing desk or at none, and a book nobody holds.
#[test]
fn adversary_w30_u4_every_order_over_a_stored_reference_agrees() {
    let inputs: Vec<Value> = [1, 2, 3, 4, 5, 6, 0x99]
        .into_iter()
        .map(|n| json!({"book_id": book(n)}))
        .collect();
    every_order_agrees(
        RETURN_BOTH,
        RETURN_BOTH_HEADER,
        RETURN_BOTH_HEAD,
        &RETURN_BOTH_PIECES,
        RETURN_TAIL,
        &inputs,
        &["withheld", "approved", "held-back"],
    );
}

/// `LendBusy`: a present-related refusal, then an external move, then a move guarded by a row set
/// over the desks, which the external move does not read.
const LEND_BUSY_COMMAND: &str = "  - name: probe.ahead.LendBusy
    input:
      - name: book_id
        type: probe.ahead.BookId
      - name: desk_id
        type: probe.ahead.DeskId
    outcomes:
      - name: no-desk
        when_related:
          via: input.desk_id
          exists: false
        error: probe.ahead.NoDesk
        summary: The named desk does not exist.
      - name: desk-closed
        when_related:
          via: input.desk_id
          predicate: state == Closed
        error: probe.ahead.DeskShut
        summary: The named desk is closed.
      - name: approved
        external: the reading room approves a quiet loan
        moves: probe.ahead.Book.lend
        instance: book_id
        emits: [probe.ahead.BookLentQuietly]
        payload:
          probe.ahead.BookLentQuietly:
            book_id: input.book_id
        summary: The book is lent quietly, as the reading room approved.
      - name: busy
        when_related:
          entity: probe.ahead.Desk
          where: state == Closed
          exists: true
        moves: probe.ahead.Book.lend
        instance: book_id
        emits: [probe.ahead.BookLent]
        payload:
          probe.ahead.BookLent:
            book_id: input.book_id
        summary: The book is lent while some desk is closed.
      - name: lent
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

fn lend_busy() -> (tempfile::TempDir, spindle_spec::EssIr) {
    with_command(LEND_BUSY, LEND_BUSY_COMMAND).expect("the fixture with LendBusy compiles")
}

/// `LendBusy` against the interpreter, unforced and forced to `approved`, books shelved and lent
/// at a missing, a closed and an open desk.
#[test]
fn adversary_w30_u4_a_row_set_after_an_external_move_agrees() {
    let (_dir, ir) = lend_busy();
    let mut differing = Vec::new();
    for n in [1, 2] {
        for desk in [NOWHERE, ELM, OAK] {
            let input = json!({"book_id": book(n), "desk_id": desk});
            for forced in [None, Some("approved")] {
                let mut steps = setup();
                steps.push((LEND_BUSY, input.clone(), forced));
                let ess = interpreted(&ir, &steps);
                let (kernel, asked) = spindled(&ir, &steps, true);
                if ess != kernel {
                    differing.push(format!(
                        "{input} forced {forced:?}: interpreter `{}`, kernel `{}` (asked {asked:?})",
                        outcome(&ess),
                        outcome(&kernel)
                    ));
                }
            }
        }
    }
    assert!(
        differing.is_empty(),
        "the kernel answers differently from the interpreter:\n  {}",
        differing.join("\n  ")
    );
}

/// What the store does at the first scan of `DESK`: a second writer opens a desk, or the read
/// fails.
enum AtScan {
    Race,
    Fail,
}

/// A memory store that acts at the kernel's first scan of `DESK`, and records the scans.
struct Watched<'s> {
    inner: MemoryStore,
    ir: &'s spindle_spec::EssIr,
    at_scan: Option<AtScan>,
    scans: Vec<String>,
}

impl Store for Watched<'_> {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.scans.push(entity.to_owned());
        if entity == DESK {
            match self.at_scan.take() {
                Some(AtScan::Fail) => {
                    return Err(StoreError::Backend("the desks cannot be read".to_owned()));
                }
                Some(AtScan::Race) => {
                    let scan = self.inner.scan(entity)?;
                    execute(
                        self.ir,
                        &mut self.inner,
                        &mut NoExternals,
                        OPEN,
                        &object(json!({"desk_id": FIR, "label": "fir"})),
                    )
                    .expect("the second writer opens a desk");
                    return Ok(scan);
                }
                None => {}
            }
        }
        self.inner.scan(entity)
    }
}

/// `LendBusy` for book 1 at the open desk, the decider taking `approved`, on a store that acts at
/// the first scan of the desks.
fn lend_approved_with(at_scan: AtScan) -> (Result<String, ExecuteError>, Vec<String>) {
    let (_dir, ir) = lend_busy();
    let mut store = Watched {
        inner: MemoryStore::default(),
        ir: &ir,
        at_scan: None,
        scans: Vec::new(),
    };
    for (command, input, _) in setup() {
        execute(&ir, &mut store, &mut NoExternals, command, &object(input))
            .unwrap_or_else(|error| panic!("setup step {command} answers: {error}"));
    }
    store.scans.clear();
    store.at_scan = Some(at_scan);
    let mut decider = |question: &External<'_>| Ok(question.outcome == "approved");
    let executed = execute(
        &ir,
        &mut store,
        &mut decider,
        LEND_BUSY,
        &object(json!({"book_id": book(1), "desk_id": OAK})),
    )
    .map(|executed| executed.outcome);
    (executed, store.scans)
}

/// The decider takes `approved`, declared before the row-set guard: the interpreter stops at it
/// and reads no row set, and so did the kernel before this unit. The walk that asks no decider now
/// scans the desks past it, so the commit of `approved` is conditioned on the desks' version, and
/// a desk opened between that scan and the commit refuses a loan that read no desk set.
#[test]
fn adversary_w30_u4_a_taken_external_move_is_not_refused_by_a_row_set_it_never_read() {
    let (executed, scans) = lend_approved_with(AtScan::Race);
    assert_eq!(
        executed.as_ref().map_err(ToString::to_string),
        Ok(&"approved".to_owned()),
        "a desk opened beside the loan refuses `approved` (scans: {scans:?})"
    );
}

/// The open point the implementor named: an error a read past an external branch raises, here a
/// store that cannot scan the desks, is returned without asking, where the decider taking
/// `approved` stops the reading before it.
#[test]
fn adversary_w30_u4_a_failing_read_past_a_taken_external_move_is_not_reached() {
    let (executed, scans) = lend_approved_with(AtScan::Fail);
    assert_eq!(
        executed.as_ref().map_err(ToString::to_string),
        Ok(&"approved".to_owned()),
        "a read of the desks after `approved` answers the request (scans: {scans:?})"
    );
}
