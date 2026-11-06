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
