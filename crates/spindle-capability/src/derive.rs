//! Deriving requirements from the compiled IR.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedComponent, ResolvedCondition};
use ess_domain::component::Reach;
use ess_domain::view::Consistency;

/// A capability name: `store.durable`, `timer`, `external.<command>.<outcome>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct Capability(String);

impl Capability {
    /// Durable state for the entities a component owns.
    pub const STORE_DURABLE: &str = "store.durable";
    /// A periodic cause: something has to fire on a schedule.
    pub const TIMER: &str = "timer";
    /// A reading of the current time, which every periodic cause needs to place its ticks.
    pub const CLOCK: &str = "clock";
    /// Commands issued from outside the process, over the network.
    pub const INGRESS_NETWORK: &str = "ingress.network";
    /// Commands issued by a person at a terminal.
    pub const INGRESS_COMMAND_LINE: &str = "ingress.command_line";
    /// An event of one component that drives a command of another.
    pub const CHANNEL: &str = "channel";
    /// A view allowed to lag the commands that change it.
    pub const PROJECTION: &str = "projection";
    /// The prefix of an outcome decided outside the model.
    pub const EXTERNAL_PREFIX: &str = "external.";

    /// A capability with this name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Its name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a capability is required: the declaration that needs it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Reason {
    /// The kind of declaration: `entity`, `binding`, `component`, `view`, `outcome`.
    pub kind: &'static str,
    /// Its name in the specification.
    pub name: String,
    /// What about it needs the capability.
    pub because: String,
}

/// Every capability a specification, or one component of it, requires, with the declarations
/// that require it.
pub type Requirements = BTreeMap<Capability, BTreeSet<Reason>>;

/// Derives what `ir` requires from the runtime.
///
/// With `component`, only what that component needs when it runs on its own: the entities of the
/// domains it owns, the commands it accepts, the bindings that start or end at it. A binding to or
/// from another component then needs a `channel`; a binding inside one component needs nothing,
/// because the runtime dispatches it in process. Without `component`, the whole system is one
/// deployment and only a binding between two components needs a channel.
///
/// Returns `None` when `component` names no component of the specification.
pub fn derive(ir: &EssIr, component: Option<&str>) -> Option<Requirements> {
    let selected = match component {
        Some(name) => Some(
            ir.components()
                .values()
                .find(|candidate| candidate.name.as_str() == name)?,
        ),
        None => None,
    };
    let mut requirements = Requirements::new();
    let mut need = |capability: &str, kind: &'static str, name: String, because: String| {
        requirements
            .entry(Capability::new(capability))
            .or_default()
            .insert(Reason {
                kind,
                name,
                because,
            });
    };

    let owned_domains = selected.map(|component| {
        component
            .owns
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>()
    });
    let in_scope_domain = |domain: String| {
        owned_domains
            .as_ref()
            .is_none_or(|owned| owned.contains(&domain))
    };
    let accepts = |command: &str| {
        selected.is_none_or(|component| {
            component
                .accepts
                .iter()
                .any(|accepted| accepted.to_string() == command)
        })
    };

    for entity in ir.entities().values() {
        if in_scope_domain(entity.domain.to_string()) {
            need(
                Capability::STORE_DURABLE,
                "entity",
                entity.name.to_string(),
                "its instances outlive the command that changed them".to_owned(),
            );
        }
    }

    for component in ir.components().values() {
        if selected.is_some_and(|chosen| chosen.name != component.name) {
            continue;
        }
        match component.reached_by {
            Reach::Network => need(
                Capability::INGRESS_NETWORK,
                "component",
                component.name.as_str().to_owned(),
                "reached_by: network".to_owned(),
            ),
            Reach::CommandLine => need(
                Capability::INGRESS_COMMAND_LINE,
                "component",
                component.name.as_str().to_owned(),
                "reached_by: command_line".to_owned(),
            ),
            Reach::InProcess => {}
        }
    }

    for command in ir.commands().values() {
        let name = command.name.to_string();
        if !accepts(&name) {
            continue;
        }
        for outcome in &command.outcomes {
            let cause = match &outcome.condition {
                ResolvedCondition::External { cause }
                | ResolvedCondition::ExternalWhen { cause, .. }
                | ResolvedCondition::ExternalSubject { cause, .. } => cause,
                _ => continue,
            };
            need(
                &format!("{}{name}.{}", Capability::EXTERNAL_PREFIX, outcome.name),
                "outcome",
                format!("{name}/{}", outcome.name),
                cause.clone(),
            );
        }
    }

    for view in ir.views().values() {
        if view.consistency == Consistency::Eventual && in_scope_domain(view.domain.to_string()) {
            need(
                Capability::PROJECTION,
                "view",
                view.name.to_string(),
                "consistency: eventual".to_owned(),
            );
        }
    }

    for binding in ir.bindings().values() {
        let command = binding.command.to_string();
        let target = owner_accepting(ir, &command);
        let binding_name = binding.name.as_str().to_owned();
        if binding.cause.periodic().is_some() {
            if accepts(&command) {
                need(
                    Capability::TIMER,
                    "binding",
                    binding_name.clone(),
                    format!("a periodic cause invokes {command}"),
                );
                need(
                    Capability::CLOCK,
                    "binding",
                    binding_name,
                    "a periodic cause is placed on the clock".to_owned(),
                );
            }
            continue;
        }
        let Some(event) = binding.cause.event() else {
            continue;
        };
        let event = event.to_string();
        let source = owner_publishing(ir, &event);
        let crosses = match (source, target) {
            (Some(source), Some(target)) => source.name != target.name,
            _ => false,
        };
        let touches = selected.is_none_or(|chosen| {
            source.is_some_and(|source| source.name == chosen.name)
                || target.is_some_and(|target| target.name == chosen.name)
        });
        if crosses && touches {
            need(
                Capability::CHANNEL,
                "binding",
                binding_name,
                format!("{event} crosses to {command} in another component"),
            );
        }
    }

    Some(requirements)
}

fn owner_publishing<'ir>(ir: &'ir EssIr, event: &str) -> Option<&'ir ResolvedComponent> {
    ir.components().values().find(|component| {
        component
            .publishes
            .iter()
            .any(|published| published.to_string() == event)
    })
}

fn owner_accepting<'ir>(ir: &'ir EssIr, command: &str) -> Option<&'ir ResolvedComponent> {
    ir.components().values().find(|component| {
        component
            .accepts
            .iter()
            .any(|accepted| accepted.to_string() == command)
    })
}
