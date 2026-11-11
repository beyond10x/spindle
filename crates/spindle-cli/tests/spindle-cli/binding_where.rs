//! A binding with a payload condition (`when: {event, where: event.kind == box}`) starts, its host
//! invokes the command for a payload that matches and nothing for one that does not, reporting
//! nothing as failed, as ESS 0.90.0's interpreter does on the same specification; and `conform`
//! runs ESS's suite for it, every scenario the in-process target can observe ending as the
//! interpreter ends it (story:binding-where-condition). `run` reading stdin carries no binding, so
//! the host is driven directly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, Deadline, InvocationObservationRequest, Runner,
    ScenarioContext, SemanticCommandRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, SystemClock};

const PARCELS: &str = "crates/spindle-cli/tests/fixtures/binding-where";
const LAPTOP: &str = "examples/environments/laptop.toml";
const RECEIVE: &str = "parcels.intake.ReceiveParcel";
const BINDING: &str = "send-boxes";
const SEND_ON: &str = "parcels.dispatch.SendOn";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parcels() -> EssIr {
    spindle_spec::load(&root().join(PARCELS))
        .expect("the fixture loads")
        .ir
}

fn spindle(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs")
}

/// What one `ReceiveParcel` led to: each (binding, command) a binding invoked, and whether the
/// command itself was answered without an error.
type Received = (Vec<(String, String)>, bool);

/// [`Received`] on ESS's interpreter, each parcel in a scenario of its own.
fn interpreted(parcel: &str, kind: &str) -> Received {
    let target = Interpreted::for_model(parcels());
    let context = ScenarioContext::new(
        "parcels.intake/authored/binding-where"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new(parcel).expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let result = target.execute_command(SemanticCommandRequest {
        command: CommandRef::new(RECEIVE.parse().expect("a command name")),
        actor: None,
        caller: None,
        input: BTreeMap::from([
            ("parcel_id".to_owned(), Node::Text(parcel.to_owned())),
            ("kind".to_owned(), Node::Text(kind.to_owned())),
        ]),
        correlation: context.correlation.clone(),
    });
    let answered = result.is_ok_and(|result| result.error.is_none());
    let invoked = target
        .observe_invocations(InvocationObservationRequest {
            binding: BINDING.parse::<BindingRef>().expect("a binding"),
            command: CommandRef::new(SEND_ON.parse().expect("a command name")),
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter answers its invocations")
        .iter()
        .map(|invocation| {
            (
                invocation.binding.to_string(),
                invocation.command.to_string(),
            )
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    (invoked, answered)
}

/// [`Received`] on the in-process host; answered means nothing was reported as failed or
/// refused.
fn hosted(host: &mut Host, parcel: &str, kind: &str) -> Received {
    let Value::Object(input) = json!({ "parcel_id": parcel, "kind": kind }) else {
        unreachable!("an object")
    };
    let report = host.submit(RECEIVE, input).expect("the parcel is received");
    let invoked = report
        .ran
        .iter()
        .filter_map(|ran| match &ran.cause {
            Cause::Delivered { binding } => Some((binding.clone(), ran.command.clone())),
            _ => None,
        })
        .collect();
    (
        invoked,
        report.failed.is_empty() && report.refused.is_empty(),
    )
}

#[test]
fn the_host_invokes_the_bound_command_only_for_a_payload_that_matches_as_ess_does() {
    let text = std::fs::read_to_string(root().join(LAPTOP)).expect("the laptop environment");
    let laptop = spindle_conformance::in_memory(
        &Environment::parse(&text).expect("the laptop environment parses"),
    );
    let mut host = Host::new(parcels(), &laptop, Arc::new(SystemClock), Path::new(""))
        .expect("a specification with a `where` condition starts");

    let sent = (vec![(BINDING.to_owned(), SEND_ON.to_owned())], true);
    let box_parcel = hosted(&mut host, "p-1", "box");
    assert_eq!(box_parcel, sent, "a box is sent on");
    assert_eq!(box_parcel, interpreted("p-1", "box"));

    let letter = hosted(&mut host, "p-2", "letter");
    assert_eq!(
        letter,
        (vec![], true),
        "a letter invokes nothing and nothing is reported as failed"
    );
    assert_eq!(letter, interpreted("p-2", "letter"));
    host.stop();
}

/// Whether every check of `scenario` that did not pass is an invocation the in-process target
/// cannot observe: it reports no invocation its bindings made (`ESS-CF-INVOCATION`).
fn unobserved_invocations_only(scenario: &Value) -> bool {
    scenario["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .filter(|check| check["status"] != "passed")
        .all(|check| check["status"] == "unsupported" && check["code"] == "ESS-CF-INVOCATION")
}

#[test]
fn every_scenario_the_target_observes_ends_as_ess_interprets_it() {
    let ir = parcels();
    let suite = ess_conformance::synthesize::synthesize(&ir).suite;
    let admitted = AdmittedSuite::from_suite(&suite).expect("ESS admits its own suite");
    let ess = Runner::for_suite(admitted.suite())
        .run_admitted(&admitted, &Interpreted::for_model(ir))
        .into_report()
        .scenarios
        .into_iter()
        .map(|result| {
            let status = serde_json::to_value(result.status).expect("a status is JSON");
            (
                result.scenario.to_string(),
                status.as_str().expect("a status is a word").to_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for id in [
        "send-boxes/binding/condition-false",
        "send-boxes/binding/flow",
        "send-boxes/binding/mapping",
    ] {
        assert_eq!(ess.get(id).map(String::as_str), Some("passed"), "{ess:#?}");
    }

    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let results = scratch.path().join("results.json");
    let report = scratch.path().join("report.json");
    let output = spindle(&[
        "conform",
        PARCELS,
        "--environment",
        LAPTOP,
        "--report",
        report.to_str().expect("utf-8"),
        "--results",
        results.to_str().expect("utf-8"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results: Value =
        serde_json::from_str(&std::fs::read_to_string(&results).expect("the results were written"))
            .expect("the results are JSON");
    let scenarios = results["scenarios"].as_array().expect("scenarios");
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario["scenario"].as_str().expect("an id"))
            .collect::<Vec<_>>(),
        ess.keys().map(String::as_str).collect::<Vec<_>>(),
        "the same suite"
    );
    for scenario in scenarios {
        let id = scenario["scenario"].as_str().expect("an id");
        let status = scenario["status"].as_str().expect("a status");
        assert!(
            status == ess[id] || (status == "unsupported" && unobserved_invocations_only(scenario)),
            "{id}: {status} here, {} on ESS's interpreter: {scenario:#}",
            ess[id]
        );
    }
    let flow = scenarios
        .iter()
        .find(|scenario| scenario["scenario"] == "send-boxes/binding/flow")
        .expect("the flow scenario");
    assert_eq!(flow["status"], "passed", "{flow:#}");
}
