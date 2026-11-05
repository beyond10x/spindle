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
            }
        })
        .collect();
    (ir.system().to_string(), components, bindings)
}
