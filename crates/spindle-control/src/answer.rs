//! The answers of the observe API, in the shapes `spindle.observe` declares
//! (`ess/domains/observe.yaml`), and the system part of `GET /api/deployment` read from the IR.

use ess_compiler::EssIr;
use serde::Serialize;
use spindle_target::{Resources, TargetKind};

/// `spindle.observe.Deployment`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Deployment {
    /// The system identifier of the deployed specification.
    pub system: String,
    /// Every component, in name order.
    pub components: Vec<Component>,
    /// Every binding, in name order.
    pub bindings: Vec<Binding>,
    /// Every instance of the placement, in document order.
    pub instances: Vec<DeployedInstance>,
}

/// `spindle.observe.Component`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Component {
    /// Its name.
    pub name: String,
    /// The domains it owns.
    pub domains: Vec<String>,
    /// The commands it accepts.
    pub commands: Vec<String>,
    /// The views of the domains it owns.
    pub views: Vec<String>,
}

/// `spindle.observe.BindingKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingKind {
    /// Fires on a schedule.
    Periodic,
    /// Carries an event.
    Event,
}

/// `spindle.observe.Binding`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Binding {
    /// Its name.
    pub name: String,
    /// Periodic or event.
    pub kind: BindingKind,
    /// The component that publishes the event; none for a periodic binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The component that accepts the command.
    pub to: String,
    /// The event carried; none for a periodic binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// The command it issues.
    pub command: String,
    /// ESS's `when.periodic.every`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,
    /// ESS's `when.periodic.anchor`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// ESS's `when.periodic.first`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
    /// ESS's `when.periodic.cadence`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cadence: Option<String>,
    /// ESS's `when.periodic.overlap`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlap: Option<String>,
    /// ESS's `when.periodic.missed`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missed: Option<String>,
    /// ESS's `when.periodic.lifetime`; none for an event binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifetime: Option<String>,
}

/// `spindle.placement.Instance.State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InstanceState {
    /// Planned onto a target, not yet decided.
    Planned,
    /// Placed: it may be launched.
    Placed,
    /// Refused by placement or by its launcher; never runs.
    Refused,
    /// Launched and running.
    Running,
    /// It ended.
    Exited,
}

/// `spindle.observe.DeployedInstance`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployedInstance {
    /// `spindle.placement.InstanceId`.
    pub instance_id: String,
    /// The component it runs.
    pub component: String,
    /// The kind of its target.
    pub target_kind: TargetKind,
    /// Its target.
    pub target: String,
    /// Its state.
    pub state: InstanceState,
    /// What it asked for, as it asked.
    pub requested: Resources,
    /// When it was launched; none until it runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

/// `spindle.observe.StateChange`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateChange {
    /// The instance.
    pub instance_id: String,
    /// The state it left.
    pub from: InstanceState,
    /// The state it entered.
    pub to: InstanceState,
    /// When, RFC 3339 in UTC.
    pub at: String,
}

/// The system part of the deployment answer: its components and bindings, read from `ir`.
pub fn system(ir: &EssIr) -> (String, Vec<Component>, Vec<Binding>) {
    let components = ir
        .components()
        .values()
        .map(|component| {
            let domains = component
                .owns
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            Component {
                name: component.name.as_str().to_owned(),
                commands: component.accepts.iter().map(ToString::to_string).collect(),
                views: ir
                    .views()
                    .values()
                    .filter(|view| component.owns.contains(&view.domain))
                    .map(|view| view.name.to_string())
                    .collect(),
                domains,
            }
        })
        .collect();
    let accepting = |command: &str| {
        ir.components()
            .values()
            .find(|component| {
                component
                    .accepts
                    .iter()
                    .any(|accepted| accepted.to_string() == command)
            })
            .map(|component| component.name.as_str().to_owned())
    };
    let publishing = |event: &str| {
        ir.components()
            .values()
            .find(|component| {
                component
                    .publishes
                    .iter()
                    .any(|published| published.to_string() == event)
            })
            .map(|component| component.name.as_str().to_owned())
    };
    let bindings = ir
        .bindings()
        .values()
        .map(|binding| {
            let command = binding.command.to_string();
            let event = binding.cause.event().map(ToString::to_string);
            let periodic = binding
                .cause
                .periodic()
                .and_then(|cause| serde_json::to_value(cause).ok());
            let word = |key: &str| {
                periodic
                    .as_ref()
                    .and_then(|cause| cause.get(key))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            };
            Binding {
                name: binding.name.as_str().to_owned(),
                kind: if event.is_some() {
                    BindingKind::Event
                } else {
                    BindingKind::Periodic
                },
                from: event.as_deref().and_then(publishing),
                to: accepting(&command).unwrap_or_default(),
                event,
                command,
                every: word("every"),
                anchor: word("anchor"),
                first: word("first"),
                cadence: word("cadence"),
                overlap: word("overlap"),
                missed: word("missed"),
                lifetime: word("lifetime"),
            }
        })
        .collect();
    (ir.system().to_string(), components, bindings)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::system;

    #[test]
    fn a_periodic_binding_publishes_the_words_of_its_schedule() {
        let rally = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess");
        let loaded = spindle_spec::load(&rally).expect("the rally example compiles");
        let (_, _, bindings) = system(&loaded.ir);
        let tick = bindings
            .iter()
            .find(|binding| binding.name == "tick")
            .expect("the tick binding");
        let answer = serde_json::to_value(tick).expect("a binding serializes");
        assert_eq!(answer["every"], "PT1S");
        assert_eq!(answer["anchor"], "host_activation");
        assert_eq!(answer["first"], "after_period");
        assert_eq!(answer["cadence"], "fixed_rate");
        assert_eq!(answer["overlap"], "serial_per_instance");
        assert_eq!(answer["missed"], "one_pending_drop_excess");
        assert_eq!(answer["lifetime"], "host_instance");
        let event = bindings
            .iter()
            .find(|binding| binding.event.is_some())
            .expect("an event binding");
        let answer = serde_json::to_value(event).expect("a binding serializes");
        for key in [
            "every", "anchor", "first", "cadence", "overlap", "missed", "lifetime",
        ] {
            assert!(answer.get(key).is_none(), "{key} in {answer}");
        }
    }
}
