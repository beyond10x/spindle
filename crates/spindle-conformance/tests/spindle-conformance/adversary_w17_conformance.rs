//! Adversarial cases for wave 17's conformance integration: the library suite is the suite
//! `ess conform synthesize --path ess --scenarios ess` writes; the scenario the CHANGELOG says
//! passes does pass; `synthesize` returns `SynthesisError::Authored` only where a listed scenario
//! was refused; and the counts gate catches a scenario that stops executing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use ess_conformance::{
    AdmittedSuite, ConformanceTarget, EventObservationRequest, ExternalOutcomeControl,
    ImplementationIdentity, ObservedEvent, RedeliveryRequest, ScenarioContext,
    SemanticCommandRequest, SemanticCommandResult, SemanticViewRequest, SemanticViewResult, Status,
    TargetError,
};
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::{Counts, HostTarget, SynthesisError};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("adversary_w17_conformance")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    Environment::parse(&text).expect("the laptop environment parses")
}

/// The library's suite of `ess/` is the suite `task check:conformance` has ESS write, byte for
/// byte (the claim of `spindle_conformance::synthesize`'s doc and the CHANGELOG).
#[test]
fn the_library_suite_is_the_suite_ess_writes() {
    let dir = scratch("suite");
    let out = dir.join("suite.json");
    let status = Command::new("ess")
        .current_dir(root())
        .args([
            "conform",
            "synthesize",
            "--path",
            "ess",
            "--scenarios",
            "ess",
            "--out",
        ])
        .arg(&out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    // The Gate lane installs `ess` and runs this comparison; a lane without it (macOS) has
    // nothing to compare against.
    let status = match status {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipped: no `ess` on PATH to compare the library suite with");
            return;
        }
        status => status.expect("ess runs"),
    };
    assert!(status.success(), "ess conform synthesize exits 0");
    let written = std::fs::read_to_string(&out).expect("ess wrote the suite");

    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let library = synthesis
        .suite
        .to_canonical_json()
        .expect("the suite serialises");

    let ids = |text: &str| {
        let value: serde_json::Value = serde_json::from_str(text).expect("JSON");
        value["scenarios"]
            .as_object()
            .map(|scenarios| scenarios.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    assert_eq!(ids(&library), ids(&written), "the scenario ids differ");
    assert_eq!(
        library.trim_end(),
        written.trim_end(),
        "the library suite differs from the one ess writes"
    );
}

/// CHANGELOG.md (Unreleased, Changed): `spindle.placement.LaunchInstance/outcome/not-admitted`,
/// the one scenario the removed declaration named, "is no longer synthesised: ESS's synthesis
/// refuses it (`ESS-SYNTH-001`)", and the outcome "is covered by the authored scenario
/// `spindle.placement/authored/launch-not-admitted`, which passes". Run the suite the task runs
/// and find each of those claims so.
#[test]
fn the_scenario_the_changelog_says_passes_passes() {
    const DECLARED: &str = "spindle.placement.LaunchInstance/outcome/not-admitted";
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let run = spindle_conformance::execute(
        &suite,
        &HostTarget::new(loaded.ir.clone(), spindle_conformance::in_memory(&laptop())),
    );
    let status = |id: &str| {
        run.report()
            .scenarios
            .iter()
            .find(|result| result.scenario.to_string() == id)
            .map(|result| result.status)
    };
    let refused = synthesis
        .refusals
        .iter()
        .map(|refusal| refusal.to_string())
        .filter(|refusal| refusal.contains(DECLARED))
        .collect::<Vec<_>>();
    assert!(
        refused
            .iter()
            .any(|refusal| refusal.starts_with("refusal[ESS-SYNTH-001]")),
        "CHANGELOG says synthesis refuses {DECLARED} as ESS-SYNTH-001: {refused:?}"
    );
    assert_eq!(
        status(DECLARED),
        None,
        "CHANGELOG says {DECLARED} is no longer synthesised"
    );
    assert_eq!(
        status(AUTHORED),
        Some(Status::Passed),
        "CHANGELOG says {AUTHORED} covers the outcome and passes; the run holds it as {:?}",
        status(AUTHORED)
    );
}

/// `synthesize`'s doc: "# Errors: When ESS refuses a listed scenario, or one has the id of a
/// scenario already in the suite." A specification that lists no scenario cannot meet either, so
/// it is never `SynthesisError::Authored` ("ESS refused the listed scenarios").
#[test]
fn a_specification_listing_no_scenario_is_not_refused_for_its_listed_scenarios() {
    let dir = scratch("binary64");
    let spec = dir.join("notes.yaml");
    std::fs::write(
        &spec,
        "format: ess/31
system: demo
version: v1
domain: demo.notes
summary: A note is added; the writer carries a Binary64 attribute.
types:
  - {name: demo.notes.NoteId, kind: newtype, of: Uuid}
entities:
  - name: demo.notes.Note
    identity: {name: note_id, type: demo.notes.NoteId}
    fields:
      - {name: label, type: String}
    lifecycle: {initial: Open, states: [Open], terminal: [Open], transitions: []}
events:
  - name: demo.notes.NoteAdded
    fields: [{name: note_id, type: demo.notes.NoteId}]
actors:
  - name: demo.notes.Writer
    attributes: [{name: rate, type: Binary64}]
    may: [demo.notes.AddNote]
commands:
  - name: demo.notes.AddNote
    input:
      - {name: label, type: String}
    outcomes:
      - name: added
        creates: demo.notes.Note
        instance: note_id
        sets: {label: input.label}
        emits: [demo.notes.NoteAdded]
        payload: {demo.notes.NoteAdded: {note_id: {generated: true}}}
views:
  - name: demo.notes.Notes
    source: demo.notes.Note
    consistency: read_your_writes
    fields:
      - {name: note_id, type: demo.notes.NoteId}
      - {name: label, type: String}
",
    )
    .expect("write the specification");
    let loaded = spindle_spec::load(&spec).expect("the specification compiles");
    assert!(
        loaded.scenarios.is_empty(),
        "a single file lists no scenario"
    );
    match spindle_conformance::synthesize(&loaded) {
        Err(SynthesisError::Authored(refusals)) => panic!(
            "no scenario is listed, and synthesize says ESS refused the listed scenarios: {refusals:?}"
        ),
        Err(other) => panic!("{other}"),
        Ok(_) => {}
    }
}

/// [`HostTarget`], except that it cannot carry out the authored scenario: every command of it is
/// unsupported, so the scenario stops executing.
struct Unsupporting {
    host: HostTarget,
    current: Mutex<Option<String>>,
}

const AUTHORED: &str = "spindle.placement/authored/launch-not-admitted";

impl ConformanceTarget for Unsupporting {
    fn identity(&self) -> Result<ImplementationIdentity, TargetError> {
        self.host.identity()
    }

    fn fixture_values(
        &self,
        scenario: &ScenarioContext,
        contract: &ess_conformance::fixtures::Contract,
    ) -> Result<BTreeMap<String, Node>, TargetError> {
        self.host.fixture_values(scenario, contract)
    }

    fn establish_entity(
        &self,
        request: ess_conformance::target::EntitySetupRequest,
    ) -> Result<(), TargetError> {
        self.host.establish_entity(request)
    }

    fn begin_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        *self.current.lock().expect("the lock") = Some(scenario.scenario.to_string());
        self.host.begin_scenario(scenario)
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        if self.current.lock().expect("the lock").as_deref() == Some(AUTHORED) {
            return Err(TargetError::Unsupported {
                observation: request.command.to_string(),
                why: "the adversary's mutant cannot carry it out".to_owned(),
            });
        }
        self.host.execute_command(request)
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        self.host.query_view(request)
    }

    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        self.host.observe_events(request)
    }

    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        self.host.configure_external_outcome(request)
    }

    fn redeliver_event(&self, request: RedeliveryRequest) -> Result<(), TargetError> {
        self.host.redeliver_event(request)
    }

    fn end_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.host.end_scenario(scenario)
    }
}

/// A target that stops executing the authored scenario fails the gate as the task runs it: the
/// executed set and the counts both name the drop, though every scenario that ran passed.
#[test]
fn a_scenario_that_stops_executing_fails_the_counts_gate() {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let committed = || {
        Counts::committed(
            &std::fs::read_to_string(root().join("conformance.toml"))
                .expect("the committed counts"),
        )
        .expect("the counts read")
    };
    let run = spindle_conformance::execute(
        &suite,
        &Unsupporting {
            host: HostTarget::new(loaded.ir.clone(), spindle_conformance::in_memory(&laptop())),
            current: Mutex::new(None),
        },
    );
    assert!(
        !run.executed_scenarios().contains(AUTHORED),
        "the mutant stopped executing {AUTHORED}: {}",
        run.report()
    );
    let set = run.set_drops(&committed());
    assert!(
        set.iter().any(|drop| drop.contains(AUTHORED)),
        "the executed set names {AUTHORED}: {set:?}"
    );
    let drops = run.counts().drops(committed());
    assert!(
        drops.iter().any(|drop| drop.contains("skipped grew")),
        "the counts name the drop: {drops:?}"
    );
}
