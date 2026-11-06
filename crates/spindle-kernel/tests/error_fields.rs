//! An error field the refusing outcome gives no source, which no held row fills and whose type
//! the runtime does not mint, is refused by name before the command runs: a refusal would
//! otherwise answer without a field its published schema requires.

use serde_json::{Map, Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, execute, runs};

const SYSTEM: &str = "format: ess/34
system: study
version: v1
summary: A ledger that refuses an overdraft.
domains:
  - study.ledger
components:
  - component: ledger
    summary: Holds the ledger.
    owns:
      domains:
        - study.ledger
    accepts:
      commands:
        - study.ledger.Withdraw
    publishes:
      events:
        - study.ledger.Withdrawn
    reached_by: in_process
";

const LEDGER: &str = "domain: study.ledger
summary: Withdrawals, refused past the limit with the shortfall.
errors:
  - name: study.ledger.Overdrawn
    summary: The withdrawal is past the limit, naming the account and the shortfall.
    fields:
      - name: account
        type: String
      - name: shortfall
        type: Decimal
commands:
  - name: study.ledger.Withdraw
    input:
      - name: account
        type: String
      - name: amount
        type: Integer
    outcomes:
      - name: overdrawn
        when: amount > 100
        error: study.ledger.Overdrawn
        payload:
          study.ledger.Overdrawn: {account: input.account}
        summary: A withdrawal past the limit is refused.
      - name: withdrawn
        emits: [study.ledger.Withdrawn]
        payload:
          study.ledger.Withdrawn: {account: input.account}
        summary: The withdrawal is made.
events:
  - name: study.ledger.Withdrawn
    fields:
      - name: account
        type: String
";

const WITHDRAW: &str = "study.ledger.Withdraw";

fn ledger() -> spindle_spec::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/ledger.yaml".to_owned()],
        &[SYSTEM.to_owned(), LEDGER.to_owned()],
    )
    .expect("the ledger compiles")
}

fn named(error: &ExecuteError) {
    match error {
        ExecuteError::Unsupported { construct } => assert!(
            construct.contains("study.ledger.Overdrawn.shortfall") && construct.contains("Decimal"),
            "{construct}"
        ),
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn an_error_field_the_runtime_cannot_fill_is_refused_by_name_before_the_command_runs() {
    let ir = ledger();
    let command = ir
        .commands()
        .values()
        .find(|command| command.name.to_string() == WITHDRAW)
        .expect("the ledger declares Withdraw");
    named(&runs(&ir, command).expect_err("the shortfall is filled by nothing"));

    let Value::Object(input) = json!({"account": "a1", "amount": 5}) else {
        unreachable!()
    };
    let input: Map<String, Value> = input;
    named(
        &execute(
            &ir,
            &mut MemoryStore::default(),
            &mut NoExternals,
            WITHDRAW,
            &input,
        )
        .expect_err("a command the kernel does not run is refused whichever branch it would take"),
    );
}

/// `tests/fixtures/held-refusal`: `LendBook` lends a free book (`when_subject:`), refuses any
/// other with the default branch `unavailable` (`Unavailable.keeper`, no source), and answers an
/// unknown book with `no-such-book` (`NoSuchBook.book_id`, no source).
fn loans() -> spindle_spec::EssIr {
    spindle_spec::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/held-refusal"),
    )
    .expect("the held-refusal fixture compiles")
    .ir
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// ESS's served application reads an unsourced error field from the addressed row for every
/// refusal without a subject of its own in a subject-guarded command, the default branch among
/// them (`ess-synth` `determined::outcome`, `reads_held`), and leaves an `unknown_instance:`
/// refusal's unsourced fields out: there is no row to describe.
#[test]
fn a_default_refusal_of_a_subject_guarded_command_reads_its_field_from_the_row() {
    let ir = loans();
    let mut store = MemoryStore::default();
    let mut send = |command: &str, input: Value| {
        execute(&ir, &mut store, &mut NoExternals, command, &object(input))
            .unwrap_or_else(|error| panic!("{command} runs: {error}"))
    };
    send(
        "study.loan.AddBook",
        json!({"book_id": "b1", "keeper": "ada"}),
    );
    assert_eq!(
        send("study.loan.LendBook", json!({"book_id": "b1"})).outcome,
        "lent"
    );
    let refused = send("study.loan.LendBook", json!({"book_id": "b1"}));
    assert_eq!(refused.outcome, "unavailable");
    assert_eq!(
        refused.error.map(|error| error.fields),
        Some(object(json!({"keeper": "ada"}))),
        "the keeper is read from the lent book, not minted"
    );

    let unknown = send("study.loan.LendBook", json!({"book_id": "b9"}));
    assert_eq!(unknown.outcome, "no-such-book");
    assert_eq!(
        unknown.error.map(|error| error.fields),
        Some(Map::new()),
        "an unknown book is described by nothing"
    );
}
