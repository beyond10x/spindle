//! Adversary cases for story:scenario-establish-steps: an established row on the host against the
//! same row on ESS's interpreter.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, EntityRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::EntitySetupRequest;
use ess_conformance::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
    TargetError,
};
use ess_domain::entity::StateName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const FIXTURE: &str = "tests/fixtures/establish";

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn fixture_ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
        .ir
}

/// The `post` fixture with the parcel domain rewritten by `edit`.
fn variant_ir(edit: impl Fn(String) -> String) -> EssIr {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "adversary_establish-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(directory.join("domains")).expect("domains");
    for file in ["system.yaml", "domains/bag.yaml"] {
        std::fs::copy(source.join(file), directory.join(file)).expect("copy");
    }
    let parcel = std::fs::read_to_string(source.join("domains/parcel.yaml")).expect("parcel");
    let edited = edit(parcel.clone());
    assert_ne!(edited, parcel, "the edit applies");
    std::fs::write(directory.join("domains/parcel.yaml"), edited).expect("write");
    spindle_spec::load(&directory)
        .unwrap_or_else(|error| panic!("the variant loads: {error:?}"))
        .ir
}

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
}

fn context() -> ScenarioContext {
    ScenarioContext::new(
        "post.parcel/authored/adversary"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary").expect("a correlation id"),
    )
}

fn kind(error: &TargetError) -> String {
    match error {
        TargetError::Unsupported { .. } => "unsupported".to_owned(),
        TargetError::Unavailable { operation, detail } => {
            format!("unavailable {operation}: {detail}")
        }
        other => format!("other {other:?}"),
    }
}

fn establish<T: ConformanceTarget + ?Sized>(
    target: &T,
    context: &ScenarioContext,
    entity: &str,
    identity: Value,
    fields: Value,
    state: &str,
) -> Result<(), String> {
    let Value::Object(fields) = fields else {
        panic!("fields are an object");
    };
    target
        .establish_entity(EntitySetupRequest {
            entity: entity.parse::<EntityRef>().expect("an entity"),
            identity: node(identity),
            fields: fields
                .into_iter()
                .map(|(field, value)| (field, node(value)))
                .collect(),
            state: StateName::new(state).expect("a state"),
            correlation: context.correlation.clone(),
        })
        .map_err(|error| kind(&error))
}

fn send<T: ConformanceTarget + ?Sized>(
    target: &T,
    context: &ScenarioContext,
    command: &str,
    input: Value,
) -> String {
    let Value::Object(input) = input else {
        panic!("input is an object");
    };
    match target.execute_command(SemanticCommandRequest {
        command: command.parse::<CommandRef>().expect("a command"),
        actor: None,
        caller: None,
        input: input
            .into_iter()
            .map(|(field, value)| (field, node(value)))
            .collect(),
        correlation: context.correlation.clone(),
    }) {
        Ok(result) => format!(
            "{} {}",
            result
                .outcome
                .map_or_else(|| "<undeclared>".to_owned(), |o| o.outcome.to_string()),
            result
                .error
                .map_or_else(String::new, |error| error.error.to_string())
        ),
        Err(error) => format!("error: {}", kind(&error)),
    }
}

fn list<T: ConformanceTarget + ?Sized>(
    target: &T,
    context: &ScenarioContext,
    view: &str,
) -> Vec<Value> {
    target
        .query_view(SemanticViewRequest {
            view: view.parse::<ViewRef>().expect("a view"),
            params: BTreeMap::new(),
            consistency: QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .unwrap_or_else(|error| panic!("{view} is read: {error:?}"))
        .rows
        .iter()
        .map(|row| {
            Value::Object(
                row.iter()
                    .filter(|(_, value)| !matches!(value, Node::Null))
                    .map(|(field, value)| {
                        (
                            field.clone(),
                            serde_json::to_value(value).expect("a node is JSON"),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// A command that creates an instance with an identity the scenario established answers on the
/// host as it answers on ESS's interpreter.
#[test]
#[ignore = "pre-existing: a create on a held identity answers undeclared where ESS answers unavailable; backlog draft"]
fn a_create_on_an_established_identity_answers_as_the_interpreter() {
    let ir = fixture_ir();
    let host = HostTarget::new(ir.clone(), laptop());
    let interpreter = Interpreted::for_model(ir);
    let drive = |target: &dyn ConformanceTarget| {
        let context = &context();
        target.begin_scenario(context).expect("open");
        let established = establish(
            target,
            context,
            "post.parcel.Parcel",
            json!("parcel-1"),
            json!({"label": "books", "missing": false}),
            "Sent",
        );
        let answer = send(
            target,
            context,
            "post.parcel.PackParcel",
            json!({"parcel_id": "parcel-1", "label": "cups"}),
        );
        let rows = list(target, context, "post.parcel.AllParcels");
        target.end_scenario(context).expect("close");
        (established, format!("{answer} | {rows:?}"))
    };
    let spindle = drive(&host);
    let ess = drive(&interpreter);
    assert_eq!(spindle, ess, "host left, interpreter right");
}

/// Two establish steps of one identity: the second is refused on both, under the same words.
#[test]
fn a_second_establish_of_one_identity_is_refused_as_the_interpreter_refuses_it() {
    let ir = fixture_ir();
    let host = HostTarget::new(ir.clone(), laptop());
    let interpreter = Interpreted::for_model(ir);
    let drive = |target: &dyn ConformanceTarget| {
        let context = context();
        target.begin_scenario(&context).expect("open");
        let first = establish(
            target,
            &context,
            "post.parcel.Parcel",
            json!("parcel-1"),
            json!({"label": "books", "missing": false}),
            "Sent",
        );
        let second = establish(
            target,
            &context,
            "post.parcel.Parcel",
            json!("parcel-1"),
            json!({"label": "cups", "missing": true}),
            "Lost",
        );
        let rows = list(target, &context, "post.parcel.AllParcels");
        target.end_scenario(&context).expect("close");
        (first, second, rows)
    };
    assert_eq!(
        drive(&host),
        drive(&interpreter),
        "host left, interpreter right"
    );
}

/// The parcel domain with a weight, a sending time and a slip, each checked by an invariant.
fn weighed(parcel: String) -> String {
    parcel
        .replacen(
            "domain: post.parcel\n",
            "domain: post.parcel\n\
             types:\n\
             \x20 - name: post.parcel.Slip\n\
             \x20   kind: struct\n\
             \x20   fields:\n\
             \x20     - name: code\n\
             \x20       type: String\n\
             \x20     - name: printed_at\n\
             \x20       type: Optional<Timestamp>\n\
             \x20 - name: post.parcel.Code\n\
             \x20   kind: newtype\n\
             \x20   of: String\n\
             \x20   invariants: ['value != \"\"']\n",
            1,
        )
        .replacen(
            "      - name: missing\n        type: Boolean\n",
            "      - name: missing\n        type: Boolean\n\
             \x20     - name: weight\n        type: Decimal\n\
             \x20     - name: pieces\n        type: Integer\n\
             \x20     - name: sent_at\n        type: Optional<Timestamp>\n\
             \x20     - name: slip\n        type: Optional<post.parcel.Slip>\n\
             \x20     - name: code\n        type: Optional<post.parcel.Code>\n\
             \x20   invariants:\n\
             \x20     - weight >= 0\n\
             \x20     - pieces > 0\n\
             \x20     - pieces <= 10\n",
            1,
        )
        .replacen(
            "          label: input.label\n          missing: false\n",
            "          label: input.label\n          missing: false\n          weight: 1\n          pieces: 1\n",
            1,
        )
        .replacen(
            "      - name: note\n        type: Optional<String>\n  - name: post.parcel.ParcelsOnTheirWay\n",
            "      - name: note\n        type: Optional<String>\n\
             \x20     - name: weight\n        type: Decimal\n\
             \x20     - name: pieces\n        type: Integer\n\
             \x20     - name: sent_at\n        type: Optional<Timestamp>\n\
             \x20     - name: slip\n        type: Optional<post.parcel.Slip>\n\
             \x20     - name: code\n        type: Optional<post.parcel.Code>\n\
             \x20 - name: post.parcel.ParcelsOnTheirWay\n",
            1,
        )
}

/// Each row is established or refused on the host as on ESS's interpreter, and the rows established
/// read the same through the view over them.
#[test]
fn typed_rows_are_established_and_refused_as_the_interpreter_does() {
    let ir = variant_ir(weighed);
    let rows: Vec<(&str, Value, &str)> = vec![
        (
            "p-int",
            json!({"label": "a", "missing": false, "weight": 2, "pieces": 3}),
            "Sent",
        ),
        (
            "p-frac",
            json!({"label": "a", "missing": false, "weight": 0.5, "pieces": 1}),
            "Packed",
        ),
        (
            "p-text-decimal",
            json!({"label": "a", "missing": false, "weight": "0.5", "pieces": 1}),
            "Packed",
        ),
        (
            "p-negative",
            json!({"label": "a", "missing": false, "weight": -1, "pieces": 1}),
            "Packed",
        ),
        (
            "p-zero-pieces",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 0}),
            "Packed",
        ),
        (
            "p-many-pieces",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 11}),
            "Packed",
        ),
        (
            "p-frac-pieces",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1.5}),
            "Packed",
        ),
        (
            "p-sent-at",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "sent_at": "2026-01-05T09:00:00Z"}),
            "Sent",
        ),
        (
            "p-bad-time",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "sent_at": "yesterday"}),
            "Sent",
        ),
        (
            "p-slip",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "slip": {"code": "x", "printed_at": null}}),
            "Sent",
        ),
        (
            "p-slip-bare",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "slip": {"code": "x"}}),
            "Sent",
        ),
        (
            "p-code",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "code": "c"}),
            "Sent",
        ),
        (
            "p-empty-code",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "code": ""}),
            "Sent",
        ),
        (
            "p-undeclared",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1, "colour": "red"}),
            "Sent",
        ),
        (
            "p-missing-weight",
            json!({"label": "a", "missing": false, "pieces": 1}),
            "Sent",
        ),
        (
            "p-null-label",
            json!({"label": null, "missing": false, "weight": 1, "pieces": 1}),
            "Sent",
        ),
        (
            "p-terminal",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1}),
            "Delivered",
        ),
        (
            "p-nowhere",
            json!({"label": "a", "missing": false, "weight": 1, "pieces": 1}),
            "Nowhere",
        ),
    ];
    let host = HostTarget::new(ir.clone(), laptop());
    let interpreter = Interpreted::for_model(ir);
    let drive = |target: &dyn ConformanceTarget| {
        let context = context();
        target.begin_scenario(&context).expect("open");
        let answers = rows
            .iter()
            .map(|(identity, fields, state)| {
                let established = establish(
                    target,
                    &context,
                    "post.parcel.Parcel",
                    json!(identity),
                    fields.clone(),
                    state,
                )
                .map_err(|error| error.split(':').next().unwrap_or_default().to_owned());
                format!("{identity}: {established:?}")
            })
            .collect::<Vec<_>>();
        let sends = rows
            .iter()
            .map(|(identity, _, _)| {
                send(
                    target,
                    &context,
                    "post.parcel.DeliverParcel",
                    json!({"parcel_id": identity}),
                )
            })
            .collect::<Vec<_>>();
        let read = list(target, &context, "post.parcel.AllParcels");
        target.end_scenario(&context).expect("close");
        (answers, sends, read)
    };
    let spindle = drive(&host);
    let ess = drive(&interpreter);
    assert_eq!(
        spindle.0, ess.0,
        "establishing: host left, interpreter right"
    );
    assert_eq!(spindle.1, ess.1, "delivering: host left, interpreter right");
    assert_eq!(spindle.2, ess.2, "reading: host left, interpreter right");
}

/// An identity of the wrong type is refused on the host as on the interpreter.
#[test]
fn an_identity_of_the_wrong_type_is_refused_as_the_interpreter_refuses_it() {
    let ir = fixture_ir();
    let host = HostTarget::new(ir.clone(), laptop());
    let interpreter = Interpreted::for_model(ir);
    let drive = |target: &dyn ConformanceTarget| {
        let context = context();
        target.begin_scenario(&context).expect("open");
        let answers = [json!(7), json!(null), json!({"id": "x"}), json!("")]
            .into_iter()
            .map(|identity| {
                establish(
                    target,
                    &context,
                    "post.parcel.Parcel",
                    identity,
                    json!({"label": "a", "missing": false}),
                    "Sent",
                )
                .map_err(|error| error.split(':').next().unwrap_or_default().to_owned())
            })
            .collect::<Vec<_>>();
        let read = list(target, &context, "post.parcel.AllParcels");
        target.end_scenario(&context).expect("close");
        (answers, read)
    };
    assert_eq!(
        drive(&host),
        drive(&interpreter),
        "host left, interpreter right"
    );
}
