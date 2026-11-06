use std::path::{Path, PathBuf};

use spindle_capability::{Admission, Environment, Requirements, admit, derive};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn spindle_ess() -> PathBuf {
    manifest().join("../../ess")
}

fn relay() -> PathBuf {
    manifest().join("tests/fixtures/relay")
}

fn names(requirements: &Requirements) -> Vec<&str> {
    requirements
        .keys()
        .map(|capability| capability.as_str())
        .collect()
}

fn required(path: &Path, component: Option<&str>) -> Requirements {
    let loaded = spindle_spec::load(path).expect("the fixture compiles");
    derive(&loaded.ir, component).expect("the component exists")
}

fn environment(provides: &str) -> Environment {
    Environment::parse(&format!(
        "format = \"spindle-environment/1\"\nname = \"test\"\nos = \"linux\"\narch = \"x86_64\"\n\n[provides]\n{provides}"
    ))
    .expect("the environment parses")
}

#[test]
fn entities_need_a_durable_store_and_an_external_outcome_needs_a_decider() {
    let requirements = required(&spindle_ess(), None);
    assert_eq!(
        names(&requirements),
        vec![
            "external.spindle.deployment.DecideAdmission.refused",
            "external.spindle.placement.LaunchInstance.not-admitted",
            "external.spindle.placement.PlaceInstance.above-ceiling",
            "external.spindle.placement.PlaceInstance.not-admitted",
            "external.spindle.placement.PlaceInstance.target-retired",
            "external.spindle.placement.PlanInstance.not-admitted",
            "store.durable"
        ]
    );
    let store = &requirements[&spindle_capability::Capability::new("store.durable")];
    assert_eq!(store.len(), 4, "{store:?}");
}

#[test]
fn a_periodic_cause_needs_a_timer_and_a_clock_and_a_cross_component_binding_a_channel() {
    assert_eq!(
        names(&required(&relay(), None)),
        vec!["channel", "clock", "ingress.network", "timer"]
    );
}

#[test]
fn each_component_needs_only_what_it_runs() {
    assert_eq!(
        names(&required(&relay(), Some("relay-source"))),
        vec!["channel", "clock", "ingress.network", "timer"]
    );
    assert_eq!(
        names(&required(&relay(), Some("relay-sink"))),
        vec!["channel"]
    );
}

#[test]
fn an_unknown_component_is_none() {
    let loaded = spindle_spec::load(&relay()).expect("compiles");
    assert!(derive(&loaded.ir, Some("relay-nowhere")).is_none());
}

#[test]
fn admission_takes_the_most_preferred_implementation() {
    let requirements = required(&relay(), Some("relay-sink"));
    let admission = admit(
        &requirements,
        &environment("channel = [\"tcp\", \"in-process\"]\n"),
    );
    let Admission::Admitted { bindings } = admission else {
        panic!("admitted expected, got {admission:?}");
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].implementation, "tcp");
}

#[test]
fn refusal_names_every_missing_capability() {
    let requirements = required(&relay(), None);
    let admission = admit(&requirements, &environment("channel = [\"in-process\"]\n"));
    assert_eq!(
        admission,
        Admission::Refused {
            missing: ["clock", "ingress.network", "timer"]
                .into_iter()
                .map(spindle_capability::Capability::new)
                .collect()
        }
    );
}

#[test]
fn a_prefix_key_provides_every_capability_under_it_and_an_exact_key_wins() {
    let requirements = required(&spindle_ess(), None);
    let admission = admit(
        &requirements,
        &environment(
            "\"store.*\" = [\"memory\"]\n\"store.durable\" = [\"sqlite\"]\n\"external.*\" = [\"operator\"]\n",
        ),
    );
    let Admission::Admitted { bindings } = admission else {
        panic!("admitted expected, got {admission:?}");
    };
    let chosen = bindings
        .iter()
        .map(|binding| (binding.capability.as_str(), binding.implementation.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        chosen,
        vec![
            (
                "external.spindle.deployment.DecideAdmission.refused",
                "operator"
            ),
            (
                "external.spindle.placement.LaunchInstance.not-admitted",
                "operator"
            ),
            (
                "external.spindle.placement.PlaceInstance.above-ceiling",
                "operator"
            ),
            (
                "external.spindle.placement.PlaceInstance.not-admitted",
                "operator"
            ),
            (
                "external.spindle.placement.PlaceInstance.target-retired",
                "operator"
            ),
            (
                "external.spindle.placement.PlanInstance.not-admitted",
                "operator"
            ),
            ("store.durable", "sqlite"),
        ]
    );
}

#[test]
fn an_environment_in_another_format_is_refused() {
    let error = Environment::parse(
        "format = \"other/1\"\nname = \"x\"\nos = \"linux\"\narch = \"x86_64\"\n",
    )
    .expect_err("wrong format");
    assert!(
        error.to_string().contains("spindle-environment/1"),
        "{error}"
    );
}

#[test]
fn an_empty_implementation_list_is_refused() {
    let error = Environment::parse(
        "format = \"spindle-environment/1\"\nname = \"x\"\nos = \"linux\"\narch = \"x86_64\"\n[provides]\ntimer = []\n",
    )
    .expect_err("empty list");
    assert!(error.to_string().contains("timer"), "{error}");
}
