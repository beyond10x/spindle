//! Adversary, wave 26 unit U1 (`story:related-selector-rows-quantifier`,
//! `story:guard-over-absent-row-field`): a rows quantifier in a `when_related:` row-set selector
//! over the selected entity itself, negated, over an absent `Optional` field of the walked row,
//! tied by identity to an absent field of the candidate, and in a `count:` selector, each against
//! ESS's interpreter from an empty walked entity on; the commit conditioned on the walked entity
//! where it held no row and on the SQLite store; and the refusal naming an absent field where the
//! field is `null`, under a double `not`, and without the values of rows or input.

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Removal, Row, Scan, SqliteStore,
    Store, StoreError, Stored, Write, execute,
};

use super::related_guards::object;

/// The kernel's answer to each step: the outcome, or its refusal.
fn spindled(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    steps: &[(&str, Value)],
) -> Vec<Result<String, ExecuteError>> {
    steps
        .iter()
        .map(|(command, input)| {
            execute(ir, store, &mut NoExternals, command, &object(input.clone()))
                .map(|executed| executed.outcome)
        })
        .collect()
}

/// ESS 0.99.1's interpreter's answer to each step, in one scenario: the outcome, or its refusal.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<Result<String, String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "ledger.usage/authored/adversary-w26-u1"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w26-u1").expect("a correlation id"),
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

/// The kernel's answers to `steps`, each the interpreter's: the same outcome, or an
/// `undecidable` refusal where the interpreter finds the same guard `Unknown`.
fn agreed(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<Result<String, ExecuteError>> {
    let ours = spindled(ir, &mut MemoryStore::default(), steps);
    let theirs = interpreted(ir, steps);
    for (step, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => assert_eq!(ours, theirs, "step {step} ({})", steps[step].0),
            (Err(ExecuteError::Undecidable { outcome, .. }), Err(theirs)) => assert!(
                theirs.contains(&format!("`{outcome}` is Unknown")),
                "step {step}: the interpreter refuses the same guard: {theirs}"
            ),
            _ => panic!(
                "step {step} ({}): the kernel answers {ours:?}, the interpreter {theirs:?}",
                steps[step].0
            ),
        }
    }
    ours
}

/// The outcome of each answer, `?` for an undecidable refusal.
fn outcomes(answers: &[Result<String, ExecuteError>]) -> Vec<String> {
    answers
        .iter()
        .map(|answer| match answer {
            Ok(outcome) => outcome.clone(),
            Err(ExecuteError::Undecidable { .. }) => "?".to_owned(),
            Err(error) => panic!("only an undecidable guard selects nothing: {error}"),
        })
        .collect()
}

mod walks {
    use super::*;

    const OPEN: &str = "ledger.usage.OpenAccount";
    const PLACE: &str = "ledger.usage.PlaceHold";
    const LIFT: &str = "ledger.usage.LiftHold";

    const ALPHA_ONE: &str = "a1000000-0000-4000-8000-000000000001";
    const ALPHA_TWO: &str = "a1000000-0000-4000-8000-000000000002";
    const BETA: &str = "a1000000-0000-4000-8000-000000000003";
    const NOBODY: &str = "a1000000-0000-4000-8000-0000000000ff";
    const ALPHA_HOLD: &str = "b1000000-0000-4000-8000-000000000001";
    const BETA_HOLD: &str = "b1000000-0000-4000-8000-000000000002";

    fn ir() -> spindle_spec::EssIr {
        spindle_spec::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-w26-u1"),
        )
        .expect("the adversary fixture compiles")
        .ir
    }

    /// `command` for the alpha account, the beta account and an account nobody opened, as one
    /// round of charges numbered from `round`.
    fn round(command: &'static str, round: u8) -> Vec<(&'static str, Value)> {
        [ALPHA_ONE, BETA, NOBODY]
            .into_iter()
            .enumerate()
            .map(|(index, account)| {
                (
                    command,
                    json!({
                        "charge_id": format!(
                            "c1000000-0000-4000-8000-0000000000{:02x}",
                            round * 3 + u8::try_from(index).expect("small")
                        ),
                        "account_id": account,
                    }),
                )
            })
            .collect()
    }

    /// One beta account, a round with no hold ever written, a second alpha account, a round,
    /// the alpha hold with no note and the beta hold `urgent`, a round, both lifted, a round.
    fn steps(command: &'static str) -> Vec<(&'static str, Value)> {
        let mut steps = vec![
            (
                OPEN,
                json!({"account_id": ALPHA_ONE, "org": "org-alpha", "hold_ref": ALPHA_HOLD}),
            ),
            (OPEN, json!({"account_id": BETA, "org": "org-beta"})),
        ];
        steps.extend(round(command, 0));
        steps.push((OPEN, json!({"account_id": ALPHA_TWO, "org": "org-alpha"})));
        steps.extend(round(command, 1));
        steps.push((PLACE, json!({"hold_id": ALPHA_HOLD, "org": "org-alpha"})));
        steps.push((
            PLACE,
            json!({"hold_id": BETA_HOLD, "org": "org-beta", "note": "urgent"}),
        ));
        steps.extend(round(command, 2));
        steps.push((LIFT, json!({"hold_id": ALPHA_HOLD})));
        steps.push((LIFT, json!({"hold_id": BETA_HOLD})));
        steps.extend(round(command, 3));
        steps
    }

    /// The outcomes of the charges alone, in order, after both sides agreed on every step.
    fn charged(command: &'static str) -> (Vec<String>, Vec<Result<String, ExecuteError>>) {
        let ir = ir();
        let steps = steps(command);
        let answers = agreed(&ir, &steps);
        let charges = steps
            .iter()
            .zip(outcomes(&answers))
            .filter(|((name, _), _)| *name == command)
            .map(|(_, outcome)| outcome)
            .collect();
        (charges, answers)
    }

    #[test]
    fn a_walk_over_the_selected_entity_itself_decides_as_the_interpreter() {
        let (charges, _) = charged("ledger.usage.ChargeSibling");
        assert_eq!(
            charges,
            [
                "charged", "charged", "charged", "shared", "charged", "charged", "shared",
                "charged", "charged", "shared", "charged", "charged"
            ],
            "alpha has a sibling once the second alpha account is open"
        );
    }

    #[test]
    fn a_negated_walk_decides_as_the_interpreter() {
        let (charges, _) = charged("ledger.usage.ChargeFree");
        assert_eq!(
            charges,
            [
                "charged", "charged", "held", "charged", "charged", "held", "held", "held", "held",
                "charged", "charged", "held"
            ],
        );
    }

    #[test]
    fn a_walk_over_an_absent_field_of_the_walked_row_decides_as_the_interpreter() {
        let (charges, answers) = charged("ledger.usage.ChargeNote");
        assert_eq!(
            charges,
            [
                "charged", "charged", "charged", "charged", "charged", "charged", "?", "urgent",
                "charged", "?", "urgent", "charged"
            ],
            "the alpha hold has no note, so whether it is urgent is unknown"
        );
        for answer in &answers {
            if let Err(error) = answer {
                let text = error.to_string();
                for value in ["org-alpha", "org-beta", "b1000000", "a1000000"] {
                    assert!(
                        !text.contains(value),
                        "the refusal names no value of a stored row: {text}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_walk_tied_to_an_absent_field_of_the_candidate_decides_as_the_interpreter() {
        let (charges, _) = charged("ledger.usage.ChargeRef");
        assert_eq!(
            charges,
            [
                "charged",
                "charged",
                "charged",
                "charged",
                "charged",
                "charged",
                "referenced",
                "?",
                "charged",
                "charged",
                "charged",
                "charged"
            ],
            "beta references no hold, so whether it references an active one is unknown"
        );
    }

    #[test]
    fn a_walk_in_a_count_selector_decides_as_the_interpreter() {
        let (charges, _) = charged("ledger.usage.ChargeCount");
        assert_eq!(
            charges,
            [
                "charged", "charged", "charged", "charged", "charged", "charged", "crowded",
                "crowded", "crowded", "charged", "charged", "charged"
            ],
            "two alpha accounts and one beta account sit under active holds"
        );
    }
}

mod races {
    use super::*;

    const ACCOUNT: &str = "metering.usage.Account";
    const HOLD: &str = "metering.usage.Hold";
    const CHARGE: &str = "metering.usage.Charge";
    const OPEN: &str = "metering.usage.OpenAccount";
    const PLACE: &str = "metering.usage.PlaceHold";
    const CHARGE_USAGE: &str = "metering.usage.ChargeUsage";
    const NORTH_ACCOUNT: &str = "a0a0a0a0-0000-4000-8000-000000000001";
    const CHARGE_ID: &str = "c0c0c0c0-0000-4000-8000-000000000001";

    fn ir() -> spindle_spec::EssIr {
        spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/metering"))
            .expect("the metering fixture compiles")
            .ir
    }

    type Race<S> = Box<dyn FnOnce(&mut S)>;

    /// A store another writer writes to right after the kernel's first scan of `entity`.
    struct Raced<S> {
        inner: S,
        entity: &'static str,
        race: Option<Race<S>>,
    }

    impl<S: Store> Store for Raced<S> {
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
            let scan = self.inner.scan(entity)?;
            if entity == self.entity
                && let Some(race) = self.race.take()
            {
                race(&mut self.inner);
            }
            Ok(scan)
        }
    }

    /// An active hold of `north` another writer places.
    fn north_hold<S: Store>(inner: &mut S) {
        inner
            .commit(&Change {
                entity: HOLD.to_owned(),
                identity: json!("b0b0b0b0-0000-4000-8000-0000000000aa"),
                expected: None,
                row: Row {
                    state: "Active".to_owned(),
                    fields: [("org".to_owned(), json!("north"))].into_iter().collect(),
                },
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("the other writer commits");
    }

    /// `ChargeUsage` of the north account on `inner`, after `setup`, raced by a north hold placed
    /// right after the kernel's scan of the holds: what it answers and whether the charge landed.
    fn raced<S: Store + 'static>(inner: S, setup: &[(&str, Value)]) -> (String, bool) {
        let ir = ir();
        let mut store = Raced {
            inner,
            entity: HOLD,
            race: None,
        };
        for (command, input) in setup {
            execute(
                &ir,
                &mut store,
                &mut NoExternals,
                command,
                &object(input.clone()),
            )
            .expect("the setup runs");
        }
        store.race = Some(Box::new(north_hold::<S>));
        let result: Result<Executed, ExecuteError> = execute(
            &ir,
            &mut store,
            &mut NoExternals,
            CHARGE_USAGE,
            &object(json!({"charge_id": CHARGE_ID, "account_id": NORTH_ACCOUNT})),
        );
        assert!(store.race.is_none(), "the kernel scanned the holds");
        let answered = match &result {
            Ok(executed) => format!("Ok({})", executed.outcome),
            Err(ExecuteError::Store(StoreError::Conflict { entity, .. })) => {
                format!("Conflict({entity})")
            }
            Err(error) => format!("Err({error:?})"),
        };
        let landed = store
            .inner
            .load(CHARGE, &json!(CHARGE_ID))
            .expect("the store reads")
            .is_some();
        (answered, landed)
    }

    /// The north account alone, and no hold ever written: the walked entity holds no row.
    fn no_hold() -> Vec<(&'static str, Value)> {
        vec![(OPEN, json!({"account_id": NORTH_ACCOUNT, "org": "north"}))]
    }

    #[test]
    fn the_first_row_of_a_walked_entity_that_held_none_conditions_the_commit() {
        assert_eq!(
            raced(MemoryStore::default(), &no_hold()),
            (format!("Conflict({HOLD})"), false),
            "the charge decided on no hold is refused once one lands"
        );
    }

    #[test]
    fn a_write_to_the_walked_entity_conditions_the_commit_on_sqlite() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let mut setup = no_hold();
        setup.push((
            PLACE,
            json!({"hold_id": "b0b0b0b0-0000-4000-8000-000000000001", "org": "south"}),
        ));
        for steps in [no_hold(), setup] {
            let path = directory
                .path()
                .join(format!("walked-{}.sqlite", steps.len()));
            let store = SqliteStore::open(&path).expect("the store opens");
            assert_eq!(
                raced(store, &steps),
                (format!("Conflict({HOLD})"), false),
                "a hold placed between the read and the commit refuses the charge on SQLite"
            );
        }
    }

    /// The same race, the hold written right after the kernel read the accounts, which it reads
    /// after the holds: the commit is still conditioned on both.
    #[test]
    fn a_write_to_the_walked_entity_after_the_selected_scan_conditions_the_commit() {
        let ir = ir();
        let mut store = Raced {
            inner: MemoryStore::default(),
            entity: ACCOUNT,
            race: None,
        };
        for (command, input) in no_hold() {
            execute(&ir, &mut store, &mut NoExternals, command, &object(input))
                .expect("the setup runs");
        }
        store.race = Some(Box::new(north_hold::<MemoryStore>));
        let result = execute(
            &ir,
            &mut store,
            &mut NoExternals,
            CHARGE_USAGE,
            &object(json!({"charge_id": CHARGE_ID, "account_id": NORTH_ACCOUNT})),
        );
        assert!(
            matches!(&result, Err(ExecuteError::Store(StoreError::Conflict { entity, .. })) if entity == HOLD),
            "the hold placed after both reads refuses the charge: {result:?}"
        );
    }
}

mod absent {
    use super::*;

    const OPEN_TICKET: &str = "desk.ticket.OpenTicket";
    const REVIEW: &str = "desk.ticket.Review";
    const TICKET: &str = "71c70000-0000-4000-8000-0000000000a1";
    const GUARD: &str = "predicate: label == mine";

    /// The `absent-row-field` fixture with `form` as both guards, in a directory of its own.
    fn guarded(form: &str) -> (tempfile::TempDir, spindle_spec::EssIr) {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/absent-row-field");
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
        std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
        std::fs::copy(fixture.join("system.yaml"), dir.path().join("system.yaml"))
            .expect("the system copies");
        let text =
            std::fs::read_to_string(fixture.join("domains/ticket.yaml")).expect("the domain reads");
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

    /// The refusal of a review of a ticket opened with `fields`, under `form`, agreed with the
    /// interpreter.
    fn refusal(form: &str, fields: Value, note: &str) -> String {
        let (_dir, ir) = guarded(form);
        let mut open = json!({"ticket_id": TICKET});
        open.as_object_mut()
            .expect("an object")
            .extend(fields.as_object().expect("an object").clone());
        let steps = [
            (OPEN_TICKET, open),
            (REVIEW, json!({"ticket_id": TICKET, "note": note})),
        ];
        let answers = agreed(&ir, &steps);
        match &answers[1] {
            Err(error @ ExecuteError::Undecidable { .. }) => error.to_string(),
            other => panic!("`{form}` is undecidable: {other:?}"),
        }
    }

    #[test]
    fn a_field_written_null_is_named_as_absent() {
        assert_eq!(
            refusal("label == mine", json!({"label": null, "rank": 7}), "n"),
            "the guard of `desk.ticket.Review/held-refused` is undecidable: `label == mine` is \
             unknown on a `desk.ticket.Ticket` row where `label` is absent; write \
             `defined(label)` in the guard to rule such a row out"
        );
    }

    #[test]
    fn a_double_negation_names_its_absent_field() {
        let text = refusal("{not: {not: label == mine}}", json!({}), "n");
        assert!(
            text.ends_with(
                "where `label` is absent; write `defined(label)` in the guard to rule such a \
                 row out"
            ),
            "{text}"
        );
    }

    #[test]
    fn the_refusal_names_no_value_of_the_row_or_the_input() {
        let text = refusal(
            "{all: [rank > 3, flagged, label == mine]}",
            json!({"rank": 4242, "flagged": true}),
            "note-value-7731",
        );
        assert!(
            text.contains("where `label` is absent"),
            "names the absent field: {text}"
        );
        for value in ["4242", "note-value-7731", "true", "71c70000"] {
            assert!(!text.contains(value), "names no value `{value}`: {text}");
        }
    }
}
