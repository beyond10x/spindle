//! Routes read from the compiled IR.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedBinding, ResolvedBindingCondition, ResolvedMapping, ResolvedMappingValue,
    ResolvedRefusalAction, ResolvedRefusalPolicy, ResolvedTypeRef,
};
use ess_domain::binding::{Delivery, Failure};
use ess_domain::command::OutcomeName;
use ess_domain::types::Primitive;
use ess_primitives::node::Node;
use ess_primitives::predicate::Truth;
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
    /// one: each `drop`, an unbounded `retry` or `escalate`.
    pub on_refusal: Option<ResolvedRefusalPolicy>,
    /// The event an escalation publishes, where the binding escalates some failure (`on_failure:
    /// {escalate: ...}`, or the `escalate:` entry of [`Self::on_refusal`]); [`Self::escalate`]
    /// makes it. [`Self::to`] publishes it.
    pub escalation: Option<String>,
    /// The binding's payload condition (`when.where`), decided by [`Self::holds`].
    condition: Option<ResolvedBindingCondition>,
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
    /// The binding escalates into an event that the component accepting its command, which
    /// publishes the escalation, does not publish.
    #[error(
        "binding {binding}: it escalates into {event}, which {component}, the component that runs \
         its command, does not publish"
    )]
    Unpublished {
        /// The binding.
        binding: String,
        /// The escalation event.
        event: String,
        /// The component that accepts the binding's command.
        component: String,
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
    /// The binding's `where` condition is neither true nor false for the event: a comparison
    /// reads an absent Optional field, or the payload cannot be read as the event declares it.
    /// ESS calls this an unmet obligation that invokes nothing, never a skip.
    #[error("binding {binding}: the `where` condition is unknown for this event: {reason}")]
    Undecided {
        /// The binding.
        binding: String,
        /// Why it is undecided.
        reason: String,
    },
    /// The refused command's input does not make the binding's escalation event: it lacks a
    /// required field of the event, or holds one that is no value of the field's type. Nothing is
    /// published; ESS's interpreter fills a placeholder there, which a runtime does not.
    #[error("binding {binding}: the escalation event {event} is unmet: {reason}")]
    EscalationUnmet {
        /// The binding.
        binding: String,
        /// The escalation event.
        event: String,
        /// What the input lacks.
        reason: String,
    },
}

impl Router {
    /// Routes every binding with an event cause.
    ///
    /// A periodic binding is the timer's and is skipped. A binding that uses anything the channel
    /// does not carry is refused whole, so a specification never runs with part of a binding:
    /// a delivery context, a selection, a bounded retry, or a mapping source other than
    /// `event.<field>` or a literal. A policy chosen per refusal whose entries drop, retry without a
    /// bound or escalate is carried ([`Route::on_refusal`]), and so are a universal escalation
    /// ([`Route::escalate`]) and a `where` condition ([`Route::holds`]). A binding that escalates
    /// into an event the component accepting its command does not publish is
    /// [`RouteError::Unpublished`]. A mapping
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

    /// These routes with every retry carried as a drop: `on_failure: retry` as `on_failure: drop`,
    /// so an envelope whose send fails is dropped, never sent again, and each `retry` entry of a
    /// policy chosen per refusal as `drop`. Every escalation is kept: a refusal the policy
    /// escalates still answers [`Failure::Escalate`], and every other answers [`Failure::Drop`].
    #[must_use]
    pub fn dropping_retries(mut self) -> Self {
        for route in &mut self.routes {
            if route.on_failure == Failure::Retry && route.on_refusal.is_none() {
                route.on_failure = Failure::Drop;
            }
            let Some(policy) = &mut route.on_refusal else {
                continue;
            };
            for action in policy
                .refusals
                .iter_mut()
                .map(|rule| &mut rule.action)
                .chain(std::iter::once(&mut policy.fallback))
            {
                if matches!(action, ResolvedRefusalAction::Retry { .. }) {
                    *action = ResolvedRefusalAction::Drop;
                }
            }
            route.on_failure = policy.fallback.word();
            if policy
                .actions()
                .all(|action| *action == ResolvedRefusalAction::Drop)
            {
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

    /// The commands `event` maps to, one per binding it triggers whose condition holds.
    pub fn map(&self, event: &Event) -> Result<Vec<(&Route, Command)>, MappingError> {
        let mut commands = Vec::new();
        for route in self.routes.iter().filter(|route| route.event == event.name) {
            if let Some(command) = route.invoke(event)? {
                commands.push((route, command));
            }
        }
        Ok(commands)
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

    /// The escalation event of an attempt with `input` that did not take effect, where
    /// [`Self::answer`] answers it with [`Failure::Escalate`]; `None` where it answers otherwise.
    ///
    /// Its payload is each field of the event that `input` holds, as ESS's interpreter takes it
    /// from the failed attempt's input (`interpret/bindings.rs`, `escalate`), checked against the
    /// field's declared type in `ir`. An optional field `input` does not hold is left out; a
    /// required one, or a value of another type, is [`MappingError::EscalationUnmet`], where the
    /// interpreter fills a placeholder: the event is not published.
    pub fn escalate(
        &self,
        ir: &EssIr,
        refusal: Option<&str>,
        input: &Map<String, Value>,
    ) -> Option<Result<Event, MappingError>> {
        let event = match &self.on_refusal {
            Some(policy) => {
                let outcome = refusal.and_then(|refusal| OutcomeName::new(refusal).ok());
                match policy.select(outcome.as_ref()) {
                    ResolvedRefusalAction::Escalate { emits } => emits.to_string(),
                    _ => return None,
                }
            }
            None if self.on_failure == Failure::Escalate => self.escalation.clone()?,
            None => return None,
        };
        let unmet = |reason: String| MappingError::EscalationUnmet {
            binding: self.binding.clone(),
            event: event.clone(),
            reason,
        };
        let Some(declared) = ir
            .events()
            .iter()
            .find_map(|(name, declared)| (name.to_string() == event).then_some(declared))
        else {
            return Some(Err(unmet("the model declares no such event".to_owned())));
        };
        let mut fields = Map::new();
        for field in &declared.fields {
            match input.get(&field.name) {
                Some(value) => {
                    if let Err(reason) = spindle_kernel::admits(ir, &field.type_ref, value) {
                        return Some(Err(unmet(format!(
                            "the command input's {} is no value of the field: {reason}",
                            field.name
                        ))));
                    }
                    fields.insert(field.name.clone(), value.clone());
                }
                None if matches!(field.type_ref, ResolvedTypeRef::Optional { .. }) => {}
                None => {
                    return Some(Err(unmet(format!(
                        "the command input has no field {}",
                        field.name
                    ))));
                }
            }
        }
        Some(Ok(Event {
            name: event.clone(),
            fields,
        }))
    }

    /// Whether this binding runs for `event`: its `where` condition, decided on the payload before
    /// the input is mapped by ESS's own evaluator, as ESS's interpreter decides it
    /// (`interpret/bindings.rs`, `holds`). `true` for a binding without one. `false` skips the
    /// binding for this event and is no failure; a condition that is Unknown (a comparison reads
    /// an absent or `null` Optional field), or that cannot read the payload as declared, is
    /// [`MappingError::Undecided`].
    pub fn holds(&self, event: &Event) -> Result<bool, MappingError> {
        let Some(condition) = &self.condition else {
            return Ok(true);
        };
        let undecided = |reason: String| MappingError::Undecided {
            binding: self.binding.clone(),
            reason,
        };
        let mut payload = BTreeMap::new();
        for read in condition.plan.reads.values() {
            let Some(field) = read.members.first() else {
                continue;
            };
            if let Some(value) = event.fields.get(field) {
                let node = serde_json::from_value::<Node>(value.clone())
                    .map_err(|error| undecided(format!("event.{field}: {error}")))?;
                payload.insert(field.clone(), node);
            }
        }
        match condition.plan.evaluate(&payload) {
            Ok(Truth::True) => Ok(true),
            Ok(Truth::False) => Ok(false),
            Ok(Truth::Unknown) => Err(undecided(
                "a comparison reads a field the event leaves absent".to_owned(),
            )),
            Err(malformed) => Err(undecided(malformed.to_string())),
        }
    }

    /// The command this binding invokes for `event`, or `None` where its `where` condition is
    /// false ([`Self::holds`], then [`Self::map`]).
    pub fn invoke(&self, event: &Event) -> Result<Option<Command>, MappingError> {
        if self.holds(event)? {
            self.map(event).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The command this binding's mapping makes of `event`, its `where` condition aside: a
    /// caller that delivers decides [`Self::holds`] first, as [`Self::invoke`] does.
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
        if binding.selection.is_some() {
            return Err(unsupported("a selection".to_owned()));
        }
        if let Some(policy) = &binding.refusal_policy {
            for action in policy.actions() {
                if let ResolvedRefusalAction::Retry { bound: Some(_) } = action {
                    return Err(unsupported("a bounded retry in `on_refusal`".to_owned()));
                }
            }
        }
        if binding.retry.is_some() {
            return Err(unsupported("a bounded retry".to_owned()));
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
        let escalations = binding
            .refusal_policy
            .iter()
            .flat_map(ResolvedRefusalPolicy::actions)
            .filter_map(|action| match action {
                ResolvedRefusalAction::Escalate { emits } => Some(emits),
                _ => None,
            })
            .chain(binding.escalation.as_ref())
            .collect::<Vec<_>>();
        for emits in &escalations {
            let published = ir.components().values().any(|component| {
                component.name.as_str() == to && component.publishes.contains(*emits)
            });
            if !published {
                return Err(RouteError::Unpublished {
                    binding: name,
                    event: emits.to_string(),
                    component: to,
                });
            }
        }
        Ok(Some(Self {
            binding: name,
            event: event_name,
            command: command_name,
            from,
            to,
            delivery: binding.delivery,
            on_failure: binding.failure,
            on_refusal: binding.refusal_policy.clone(),
            escalation: escalations.first().map(ToString::to_string),
            condition: binding.condition.clone(),
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
