//! A value read through two rows (ess/22, `{related: {via: [input.<field>, <field>], field: …}}`):
//! the row the input names, then the row a field of that row names, read from the store as it was
//! before the outcome, as ESS's interpreter reads it (`interpret::execute::determined::Reads::
//! related`) (`story:value-source-through-two-rows`).
//!
//! The fixture `tests/fixtures/related-chain` declares formations, workloads that name a
//! formation (`home_id`, and an `Optional` `formation_id`), instances and badges. `CreateInstance`
//! copies the `List<String>` slots and the state of the formation its workload's `formation_id`
//! names; `BadgeWorkload` fills a badge's tag, a newtype of at least four characters, from the
//! code of the workload's home formation through a declared conversion, as `BadgeFormation` does
//! from a formation the input names. `LeaseInstance` reads the formation a lease of component
//! `yard` names, as `NoteLease` reads that lease's field in one hop.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::SystemTime;

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::TargetError;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Executed, Invocation, MemoryStore, NoExternals, Store, UuidV4, execute,
};

use super::related_guards::object;

const INSTANCE: &str = "forge.plant.Instance";
const BADGE: &str = "forge.plant.Badge";
const REGISTER_FORMATION: &str = "forge.plant.RegisterFormation";
const READY: &str = "forge.plant.ReadyFormation";
const REGISTER_WORKLOAD: &str = "forge.plant.RegisterWorkload";
const CREATE: &str = "forge.plant.CreateInstance";
const BADGE_WORKLOAD: &str = "forge.plant.BadgeWorkload";
const BADGE_FORMATION: &str = "forge.plant.BadgeFormation";
const REGISTER_LEASE: &str = "forge.yard.RegisterLease";
const LEASE: &str = "forge.plant.LeaseInstance";
const NOTE: &str = "forge.plant.NoteLease";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/related-chain"))
        .expect("the related-chain fixture compiles")
        .ir
}

/// What one command was answered: the outcome with the payload of every event it published, or
/// the kind of answer it was refused with, named as ESS's interpreter names it.
#[derive(Debug, PartialEq)]
enum Answer {
    Took {
        outcome: String,
        events: Vec<Value>,
    },
    /// No value determined: the kernel's `no_value`, ESS's `Undetermined::NoValue`.
    NoValue,
    /// The request is not what the model declares: the kernel's `invalid_input`, ESS's
    /// `Undetermined::Request`.
    Request,
    /// A construct not run: the kernel's `unsupported`, ESS's `Undetermined::NotInterpreted`.
    Unsupported,
    Other(String),
}

/// What a run observed: each command's answer, then every instance and badge row by identity.
#[derive(Debug, PartialEq)]
struct Observed {
    answers: Vec<Answer>,
    rows: Vec<Value>,
}

fn kernel_answer(result: Result<Executed, ExecuteError>) -> Answer {
    match result {
        Ok(executed) => Answer::Took {
            outcome: executed.outcome,
            events: executed
                .events
                .into_iter()
                .map(|event| Value::Object(event.payload.into_iter().collect()))
                .collect(),
        },
        Err(ExecuteError::NoValue { .. }) => Answer::NoValue,
        Err(ExecuteError::InvalidInput(_)) => Answer::Request,
        Err(ExecuteError::Unsupported { .. }) => Answer::Unsupported,
        Err(other) => Answer::Other(other.to_string()),
    }
}

/// `steps` on the kernel, one store; reaching every component where `reaching` is set, as a host
/// running them in one process does, else the running component's store alone.
fn kernel(
    ir: &spindle_spec::EssIr,
    steps: &[(&str, Value)],
    rows: &[(&str, &str)],
    reaching: bool,
) -> Observed {
    let every: BTreeSet<String> = ir
        .components()
        .values()
        .map(|component| component.name.as_str().to_owned())
        .collect();
    let mut store = MemoryStore::default();
    let answers = steps
        .iter()
        .map(|(command, input)| {
            let input = object(input.clone());
            kernel_answer(if reaching {
                spindle_kernel::execute_reaching(
                    ir,
                    &mut store,
                    &mut NoExternals,
                    &mut UuidV4,
                    &SystemTime::now,
                    &Invocation {
                        command,
                        input: &input,
                        actor: None,
                        caller: None,
                    },
                    &std::collections::BTreeMap::new(),
                    &every,
                )
            } else {
                execute(ir, &mut store, &mut NoExternals, command, &input)
            })
        })
        .collect();
    let rows = rows
        .iter()
        .map(|(entity, identity)| {
            store
                .load(entity, &json!(identity))
                .expect("the store reads")
                .map_or(Value::Null, |stored| {
                    let mut fields: Map<String, Value> = stored.row.fields.into_iter().collect();
                    let key = if *entity == INSTANCE {
                        "instance_id"
                    } else {
                        "badge_id"
                    };
                    fields.insert(key.to_owned(), json!(identity));
                    Value::Object(fields)
                })
        })
        .collect();
    Observed { answers, rows }
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// `steps` on ESS's interpreter, which holds every entity in one store.
fn interpreted(
    ir: &spindle_spec::EssIr,
    steps: &[(&str, Value)],
    rows: &[(&str, &str)],
) -> Observed {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "forge.plant/authored/chain".parse().expect("a scenario id"),
        CorrelationId::new("chain").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let answers = steps
        .iter()
        .map(|(command, input)| {
            let input = object(input.clone())
                .into_iter()
                .map(|(field, value)| {
                    let node: Node = serde_json::from_value(value).expect("a node");
                    (field, node)
                })
                .collect();
            match target.execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            }) {
                Ok(result) => Answer::Took {
                    outcome: result
                        .outcome
                        .map(|outcome| outcome.to_string())
                        .and_then(|outcome| {
                            outcome.rsplit_once('/').map(|(_, name)| name.to_owned())
                        })
                        .unwrap_or_default(),
                    events: result
                        .direct_events
                        .iter()
                        .map(|event| {
                            Value::Object(
                                event
                                    .payload
                                    .iter()
                                    .map(|(field, value)| (field.clone(), json_of(value)))
                                    .collect(),
                            )
                        })
                        .collect(),
                },
                Err(TargetError::Unavailable { detail, .. })
                    if detail.contains("the request is not what the model declares") =>
                {
                    Answer::Request
                }
                Err(TargetError::Unsupported { why, .. })
                    if why.contains("to the implementation") =>
                {
                    Answer::NoValue
                }
                Err(TargetError::Unsupported { .. }) => Answer::Unsupported,
                Err(other) => Answer::Other(format!("{other:?}")),
            }
        })
        .collect();
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let viewed = |view: &str| -> Vec<Value> {
        target
            .query_view(SemanticViewRequest {
                view: ViewRef::new(view.parse().expect("a view name")),
                params: std::collections::BTreeMap::new(),
                consistency: QueryConsistency::Current,
                correlation: context.correlation.clone(),
                deadline,
            })
            .unwrap_or_else(|error| panic!("{view} reads: {error:?}"))
            .rows
            .iter()
            .map(|row| {
                Value::Object(
                    row.iter()
                        .filter(|(_, value)| **value != Node::Null)
                        .map(|(field, value)| (field.clone(), json_of(value)))
                        .collect(),
                )
            })
            .collect()
    };
    let instances = viewed("forge.plant.Instances");
    let badges = viewed("forge.plant.Badges");
    let rows = rows
        .iter()
        .map(|(entity, identity)| {
            let (held, key) = if *entity == INSTANCE {
                (&instances, "instance_id")
            } else {
                (&badges, "badge_id")
            };
            held.iter()
                .find(|row| row[key] == json!(identity))
                .cloned()
                .unwrap_or(Value::Null)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    Observed { answers, rows }
}

fn formation(id: &str, code: &str, slots: &[&str]) -> (&'static str, Value) {
    (
        REGISTER_FORMATION,
        json!({"formation_id": id, "code": code, "slots": slots}),
    )
}

fn workload(id: &str, home: &str, formation: Option<&str>) -> (&'static str, Value) {
    let mut input = json!({"workload_id": id, "home_id": home});
    if let Some(formation) = formation {
        input["formation_id"] = json!(formation);
    }
    (REGISTER_WORKLOAD, input)
}

fn instance(id: &str, workload: &str) -> (&'static str, Value) {
    (CREATE, json!({"instance_id": id, "workload_id": workload}))
}

#[test]
fn a_value_source_through_two_rows_reads_the_far_row() {
    let ir = ir();
    let steps = [
        formation("frm-1", "AB-1", &["north", "south"]),
        (READY, json!({"formation_id": "frm-1"})),
        workload("wl-1", "frm-1", Some("frm-1")),
        instance("ins-1", "wl-1"),
    ];
    let rows = [(INSTANCE, "ins-1")];
    let spindle = kernel(&ir, &steps, &rows, false);
    assert_eq!(
        spindle.answers[3],
        Answer::Took {
            outcome: "created".to_owned(),
            events: vec![json!({
                "instance_id": "ins-1",
                "slots": ["north", "south"],
                "formation_state": "Ready",
            })],
        },
        "the instance is created from the formation two rows away"
    );
    assert_eq!(
        spindle.rows,
        [json!({
            "instance_id": "ins-1",
            "slots": ["north", "south"],
            "formation_state": "Ready",
        })],
        "the row holds the far row's slots and state"
    );
    assert_eq!(
        spindle,
        interpreted(&ir, &steps, &rows),
        "ESS's interpreter answers and writes the same"
    );
}

#[test]
fn a_missing_row_in_the_chain_is_answered_as_ess_answers_it() {
    let ir = ir();
    let rows = [(INSTANCE, "ins-1")];
    for (case, steps) in [
        ("an absent first row", vec![instance("ins-1", "wl-none")]),
        (
            "an absent far row",
            vec![
                workload("wl-1", "frm-none", Some("frm-none")),
                instance("ins-1", "wl-1"),
            ],
        ),
        (
            "an absent `Optional` reference on the way",
            vec![
                formation("frm-1", "AB-1", &["north"]),
                workload("wl-1", "frm-1", None),
                instance("ins-1", "wl-1"),
            ],
        ),
    ] {
        let spindle = kernel(&ir, &steps, &rows, false);
        let ess = interpreted(&ir, &steps, &rows);
        assert_eq!(spindle, ess, "{case}: answered as ESS answers it");
        let last = spindle.answers.last().expect("an answer");
        if case.contains("Optional") {
            assert_eq!(
                *last,
                Answer::Took {
                    outcome: "created".to_owned(),
                    events: vec![json!({"instance_id": "ins-1"})],
                },
                "{case}: the values are absent"
            );
            assert_eq!(spindle.rows, [json!({"instance_id": "ins-1"})], "{case}");
        } else {
            assert_eq!(*last, Answer::NoValue, "{case}: no value is determined");
            assert_eq!(spindle.rows, [Value::Null], "{case}: nothing is written");
        }
    }
}

#[test]
fn a_chain_through_another_component_s_row_is_checked_at_every_hop() {
    let ir = ir();
    let command = |name: &str| {
        ir.commands()
            .values()
            .find(|command| command.name.to_string() == name)
            .expect("the command")
    };
    let plant = BTreeSet::from(["plant".to_owned()]);
    let both = BTreeSet::from(["plant".to_owned(), "yard".to_owned()]);
    for name in [LEASE, NOTE] {
        let refused = spindle_kernel::runs(&ir, command(name))
            .expect_err("a read of the lease is refused on the plant's store alone");
        assert!(
            refused
                .to_string()
                .contains("`forge.yard.Lease`, owned by component `yard`"),
            "{name} names the lease it reads: {refused}"
        );
        assert!(
            spindle_kernel::runs_reaching(&ir, command(name), &plant).is_err(),
            "{name} is refused reaching the plant's store alone"
        );
        spindle_kernel::runs_reaching(&ir, command(name), &both)
            .unwrap_or_else(|error| panic!("{name} runs reaching the yard's store: {error}"));
    }

    let steps = [
        formation("frm-1", "AB-1", &["north"]),
        (
            REGISTER_LEASE,
            json!({"lease_id": "lse-1", "formation_id": "frm-1"}),
        ),
        (NOTE, json!({"instance_id": "ins-1", "lease_id": "lse-1"})),
        (LEASE, json!({"instance_id": "ins-2", "lease_id": "lse-1"})),
    ];
    let rows = [(INSTANCE, "ins-1"), (INSTANCE, "ins-2")];
    let split = kernel(&ir, &steps, &rows, false);
    assert_eq!(
        split.answers[2..],
        [Answer::Unsupported, Answer::Unsupported],
        "the chain is refused where the one-hop read of the lease is"
    );
    assert_eq!(split.rows, [Value::Null, Value::Null], "nothing is written");

    let one_host = kernel(&ir, &steps, &rows, true);
    assert_eq!(
        one_host.rows,
        [
            json!({"instance_id": "ins-1", "formation_id": "frm-1"}),
            json!({"instance_id": "ins-2", "slots": ["north"]}),
        ],
        "reaching the yard's store, the chain is read where the one-hop read is"
    );
    assert_eq!(
        one_host,
        interpreted(&ir, &steps, &rows),
        "as ESS's interpreter reads them"
    );
}

#[test]
fn a_chained_read_through_a_conversion_is_checked_as_a_one_hop_read() {
    let ir = ir();
    let rows = [(BADGE, "bdg-1"), (BADGE, "bdg-2")];
    for (code, answer) in [("AB-1", None), ("AB", Some(Answer::Request))] {
        let steps = [
            formation("frm-1", code, &["north"]),
            workload("wl-1", "frm-1", None),
            (
                BADGE_WORKLOAD,
                json!({"badge_id": "bdg-1", "workload_id": "wl-1"}),
            ),
            (
                BADGE_FORMATION,
                json!({"badge_id": "bdg-2", "formation_id": "frm-1"}),
            ),
        ];
        let spindle = kernel(&ir, &steps, &rows, false);
        assert_eq!(
            spindle,
            interpreted(&ir, &steps, &rows),
            "{code}: answered as ESS's interpreter answers it"
        );
        match answer {
            Some(refused) => {
                assert_eq!(
                    spindle.answers[2..],
                    [refused, Answer::Request],
                    "{code}: the chained read is refused as the one-hop read is"
                );
                assert_eq!(
                    spindle.rows,
                    [Value::Null, Value::Null],
                    "nothing is written"
                );
            }
            None => assert_eq!(
                spindle.rows,
                [
                    json!({"badge_id": "bdg-1", "tag": code}),
                    json!({"badge_id": "bdg-2", "tag": code}),
                ],
                "{code}: the code is passed through the conversion unchanged"
            ),
        }
    }
}
