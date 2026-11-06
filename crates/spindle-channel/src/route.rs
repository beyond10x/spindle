//! Routes read from the compiled IR.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedBinding, ResolvedMapping, ResolvedMappingValue, ResolvedRefusalAction,
    ResolvedRefusalPolicy, ResolvedTypeRef,
};
use ess_domain::binding::{Delivery, Failure};
use ess_domain::command::OutcomeName;
use ess_domain::types::Primitive;
use serde_json::{Map, Value};

use crate::model::{Command, Event};

/// Every event binding of a specification, routed, and the events each component publishes.
#[derive(Debug, Clone)]
pub struct Router {
    routes: Vec<Route>,
    publishes: BTreeMap<String, BTreeSet<String>>,
}

/// One event binding: which event, from which component, becomes which command in which.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The binding's identifier.
    pub binding: String,
    /// The event that triggers it.
    pub event: String,
    /// The command it invokes.
    pub command: String,
    /// The component that publishes the event.
    pub from: String,
    /// The component that accepts the command.
    pub to: String,
    /// How many times the command may run.
    pub delivery: Delivery,
    /// What happens when it does not. Under [`Self::on_refusal`] this is the fallback's word, ESS's
    /// universal view of the table, and only [`Self::answer`] says what answers one refusal.
    pub on_failure: Failure,
    /// The policy chosen per refusal of the command (`on_refusal`), where the binding declares
    /// one: each `drop` or an unbounded `retry`.
    pub on_refusal: Option<ResolvedRefusalPolicy>,
    rules: Vec<Rule>,
}

/// How one command input is filled.
#[derive(Debug, Clone, PartialEq)]
struct Rule {
    target: String,
    source: Source,
    /// The input's declared type, when the mapping crosses a declared conversion into it.
    converted: Option<Converted>,
}

/// The type a value crossing a declared conversion must be, the conversion's stated reason, and the
/// specification that declares both, shared by every route of one router. ESS carries the value unchanged and checks it against the target type before the command
/// port (`interpret/bindings.rs`, `mapped`); an assignable source needs no check.
#[derive(Clone)]
struct Converted {
    kind: ResolvedTypeRef,
    because: String,
    ir: Arc<EssIr>,
}

/// Two conversions are equal when they cross into the same type for the same stated reason.
impl PartialEq for Converted {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.because == other.because
    }
}

impl std::fmt::Debug for Converted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Converted")
            .field("kind", &self.kind)
            .field("because", &self.because)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Source {
    /// `event.<field>`; `optional` when the target may be absent.
    EventField { field: String, optional: bool },
    /// A value written in the binding, already typed against its target.
    Literal(Value),
}

/// Why a specification's bindings cannot be routed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    /// The binding uses a construct the channel does not carry.
    #[error("binding {binding}: {construct} is not supported by the channel")]
    Unsupported {
        /// The binding.
        binding: String,
        /// The construct, as the document wrote it where it can be quoted.
        construct: String,
    },
    /// Not exactly one component holds one end of the binding.
    #[error("binding {binding}: {role} {name} is held by {} components ({})", found.len(), found.join(", "))]
    Placement {
        /// The binding.
        binding: String,
        /// `publishes` or `accepts`.
        role: &'static str,
        /// The event or command.
        name: String,
        /// The components that hold it.
        found: Vec<String>,
    },
}

/// Why an event did not map onto its command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingError {
    /// A required source field is absent from the event.
    #[error("binding {binding}: the event has no field {field}")]
    MissingField {
        /// The binding.
        binding: String,
        /// The event field.
        field: String,
    },
    /// A value does not satisfy the declared type of the input it fills across a declared
    /// conversion: the binding's input is unmet, and its command is not invoked.
    #[error("binding {binding}: the input {target} is unmet: {reason}")]
    Unmet {
        /// The binding.
        binding: String,
        /// The command input.
        target: String,
        /// What the value is not.
        reason: String,
    },
}

impl Router {
    /// Routes every binding with an event cause.
    ///
    /// A periodic binding is the timer's and is skipped. A binding that uses anything the channel
    /// does not carry is refused whole, so a specification never runs with part of a binding:
    /// a delivery context, a `where` condition, a selection, a bounded retry, an escalation
    /// (universal or chosen per refusal), or a mapping source other than `event.<field>` or a
    /// literal. A policy chosen per refusal whose entries drop or retry without a bound is
    /// carried ([`Route::on_refusal`]). A mapping
    /// across a declared conversion is carried, its value checked when it is mapped, against a copy of
    /// `ir` this router holds; [`Self::from_shared`] holds no copy.
    pub fn from_ir(ir: &EssIr) -> Result<Self, RouteError> {
        Self::build(ir, || Arc::new(ir.clone()))
    }

    /// [`Self::from_ir`], checking converted values against `ir` itself rather than a copy of it.
    pub fn from_shared(ir: &Arc<EssIr>) -> Result<Self, RouteError> {
        Self::build(ir, || Arc::clone(ir))
    }

    /// The routes of `ir`; `shared` is called once, and only when some mapping crosses a
    /// conversion.
    fn build(ir: &EssIr, shared: impl FnOnce() -> Arc<EssIr>) -> Result<Self, RouteError> {
        let converts = ir.bindings().values().any(|binding| {
            binding
                .mapping
                .iter()
                .any(|field| field.conversion.is_some())
        });
        let shared = converts.then(shared);
        let mut routes = Vec::new();
        for binding in ir.bindings().values() {
            if let Some(route) = Route::compile(ir, binding, shared.as_ref())? {
                routes.push(route);
            }
        }
        let publishes = ir
            .components()
            .values()
            .map(|component| {
                (
                    component.name.as_str().to_owned(),
                    component
                        .publishes
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                )
            })
            .collect();
        Ok(Self { routes, publishes })
    }

    /// These routes with every `on_failure: retry`, and every policy chosen per refusal, carried
    /// as `on_failure: drop`: an envelope whose send fails is dropped, never sent again, and every
    /// refusal answers [`Failure::Drop`].
    #[must_use]
    pub fn dropping_retries(mut self) -> Self {
        for route in &mut self.routes {
            if route.on_failure == Failure::Retry || route.on_refusal.is_some() {
                route.on_failure = Failure::Drop;
                route.on_refusal = None;
            }
        }
        self
    }

    /// The route of the binding named `binding`.
    pub fn route(&self, binding: &str) -> Option<&Route> {
        self.routes.iter().find(|route| route.binding == binding)
    }

    /// Every route, in binding-name order.
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// The events `component` publishes; `None` when the specification has no such component.
    pub fn publishes(&self, component: &str) -> Option<&BTreeSet<String>> {
        self.publishes.get(component)
    }

    /// The commands `event` maps to, one per binding it triggers.
    pub fn map(&self, event: &Event) -> Result<Vec<(&Route, Command)>, MappingError> {
        self.routes
            .iter()
            .filter(|route| route.event == event.name)
            .map(|route| route.map(event).map(|command| (route, command)))
            .collect()
    }
}

impl Route {
    /// What answers an attempt of this binding's command that did not take effect: refused with
    /// the declared outcome `refusal`, or, with `None`, failed with no declared outcome. Under
    /// [`Self::on_refusal`] it is the refusal's policy, else the table's fallback, as ESS's
    /// [`ResolvedRefusalPolicy::select`] chooses; otherwise [`Self::on_failure`].
    pub fn answer(&self, refusal: Option<&str>) -> Failure {
        let Some(policy) = &self.on_refusal else {
            return self.on_failure;
        };
        let outcome = refusal.and_then(|refusal| OutcomeName::new(refusal).ok());
        policy.select(outcome.as_ref()).word()
    }

    /// Whether some attempt that does not take effect is retried: under `on_failure: retry`, or
    /// where a policy chosen per refusal retries a refusal or its fallback.
    pub fn retries(&self) -> bool {
        match &self.on_refusal {
            Some(policy) => policy
                .actions()
                .any(|action| matches!(action, ResolvedRefusalAction::Retry { .. })),
            None => self.on_failure == Failure::Retry,
        }
    }

    /// The command this binding invokes for `event`.
    ///
    /// An absent `event.<field>` source leaves an Optional input out and refuses a required one. A
    /// value carried across a declared conversion that is no value of its input's declared type
    /// is [`MappingError::Unmet`].
    pub fn map(&self, event: &Event) -> Result<Command, MappingError> {
        let mut fields = Map::new();
        for rule in &self.rules {
            let value = match &rule.source {
                Source::EventField { field, optional } => match event.fields.get(field) {
                    Some(value) => value,
                    None if *optional => continue,
                    None => {
                        return Err(MappingError::MissingField {
                            binding: self.binding.clone(),
                            field: field.clone(),
                        });
                    }
                },
                Source::Literal(value) => value,
            };
            if let Some(converted) = &rule.converted {
                spindle_kernel::admits(&converted.ir, &converted.kind, value).map_err(
                    |reason| MappingError::Unmet {
                        binding: self.binding.clone(),
                        target: rule.target.clone(),
                        reason,
                    },
                )?;
            }
            fields.insert(rule.target.clone(), value.clone());
        }
        Ok(Command {
            name: self.command.clone(),
            fields,
        })
    }

    fn compile(
        ir: &EssIr,
        binding: &ResolvedBinding,
        shared: Option<&Arc<EssIr>>,
    ) -> Result<Option<Self>, RouteError> {
        let Some(event) = binding.cause.event() else {
            return Ok(None);
        };
        let name = binding.name.as_str().to_owned();
        let unsupported = |construct: String| RouteError::Unsupported {
            binding: name.clone(),
            construct,
        };
        if binding.context.is_some() {
            return Err(unsupported("a delivery context".to_owned()));
        }
        if binding.condition.is_some() {
            return Err(unsupported("a `where` condition".to_owned()));
        }
        if binding.selection.is_some() {
            return Err(unsupported("a selection".to_owned()));
        }
        if let Some(policy) = &binding.refusal_policy {
            for action in policy.actions() {
                match action {
                    ResolvedRefusalAction::Drop | ResolvedRefusalAction::Retry { bound: None } => {}
                    ResolvedRefusalAction::Retry { bound: Some(_) } => {
                        return Err(unsupported("a bounded retry in `on_refusal`".to_owned()));
                    }
                    ResolvedRefusalAction::Escalate { .. } => {
                        return Err(unsupported("an escalation in `on_refusal`".to_owned()));
                    }
                }
            }
        }
        if binding.retry.is_some() {
            return Err(unsupported("a bounded retry".to_owned()));
        }
        if binding.failure == Failure::Escalate {
            return Err(unsupported("on_failure: escalate".to_owned()));
        }
        let rules = binding
            .mapping
            .iter()
            .map(|mapping| rule(ir, mapping, shared).map_err(&unsupported))
            .collect::<Result<Vec<_>, _>>()?;

        let event_name = event.to_string();
        let command_name = binding.command.to_string();
        let from = single(
            &name,
            "publishes",
            &event_name,
            ir.components()
                .values()
                .filter(|component| component.publishes.contains(event))
                .map(|component| component.name.as_str().to_owned()),
        )?;
        let to = single(
            &name,
            "accepts",
            &command_name,
            ir.components()
                .values()
                .filter(|component| component.accepts.contains(&binding.command))
                .map(|component| component.name.as_str().to_owned()),
        )?;
        Ok(Some(Self {
            binding: name,
            event: event_name,
            command: command_name,
            from,
            to,
            delivery: binding.delivery,
            on_failure: binding.failure,
            on_refusal: binding.refusal_policy.clone(),
            rules,
        }))
    }
}

fn single(
    binding: &str,
    role: &'static str,
    name: &str,
    found: impl Iterator<Item = String>,
) -> Result<String, RouteError> {
    let mut found = found.collect::<Vec<_>>();
    if found.len() == 1 {
        return Ok(found.remove(0));
    }
    Err(RouteError::Placement {
        binding: binding.to_owned(),
        role,
        name: name.to_owned(),
        found,
    })
}

fn rule(
    ir: &EssIr,
    mapping: &ResolvedMapping,
    shared: Option<&Arc<EssIr>>,
) -> Result<Rule, String> {
    let target = mapping.target.clone();
    let converted = match (&mapping.conversion, shared) {
        (None, _) => None,
        (Some(because), Some(ir)) => Some(Converted {
            kind: mapping.target_type.clone(),
            because: because.clone(),
            ir: Arc::clone(ir),
        }),
        (Some(_), None) => return Err(format!("the declared conversion into {target}")),
    };
    let source = match &mapping.value {
        ResolvedMappingValue::EventField { field, .. } => Source::EventField {
            field: field.clone(),
            optional: matches!(mapping.target_type, ResolvedTypeRef::Optional { .. }),
        },
        ResolvedMappingValue::Literal { value } => Source::Literal(literal(ir, mapping, value)?),
        ResolvedMappingValue::EventAccessor { plan, .. } => {
            return Err(format!("{target}: {}", plan.path()));
        }
        ResolvedMappingValue::DeliveryContext { field, .. } => {
            return Err(format!("{target}: context.{field}"));
        }
        ResolvedMappingValue::HostContext { field, .. } => {
            return Err(format!("{target}: host_context.{field}"));
        }
        ResolvedMappingValue::HostRead { field, .. } => {
            return Err(format!("{target}: host_read.{field}"));
        }
        ResolvedMappingValue::Selection { .. } => {
            return Err(format!("{target}: a selection projection"));
        }
    };
    Ok(Rule {
        target,
        source,
        converted,
    })
}

/// A literal as its target's wire form reads it ([`EssIr::literal_primitive`]): a Boolean target
/// as a JSON Boolean, an Integer target as a JSON integer, and a Decimal target as the IR's
/// canonical decimal string, never a JSON float, because ESS 0.85.0 puts a Decimal on the wire as
/// a decimal string. Everything else is the text written.
fn literal(ir: &EssIr, mapping: &ResolvedMapping, value: &str) -> Result<Value, String> {
    let refused = || format!("{}: the literal {value}", mapping.target);
    match ir.literal_primitive(&mapping.target_type) {
        Some(Primitive::Boolean) => match value {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(refused()),
        },
        Some(Primitive::Integer) => value
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| value.parse::<u64>().map(Value::from))
            .map_err(|_| refused()),
        _ => Ok(Value::String(value.to_owned())),
    }
}
