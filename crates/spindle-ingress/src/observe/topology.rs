//! `GET /__spindle/topology`: the graph of what a served run serves, a `spindle.observe.Topology`
//! (`spindle-topology/1`), with the live numbers of each edge.
//!
//! The graph is derived, never authored. Its nodes: the ingress; the specification the host runs
//! (its compiled model); each domain in name order, followed by its commands, views and events,
//! each in name order, and the external causes of its commands' outcomes; each binding in name
//! order. Its edges, kind by kind: `routes` from the ingress to the command or view of each route
//! the surface has, also one answered `501`, in the order of the nodes they reach
//! ([`Recorder::routes`]); `contains` from the specification to each domain and from each domain
//! to what it declares; `asks` from a command to each external cause of its outcomes; `emits`
//! from a command to each event its outcomes emit; `carries` from an event to each event binding
//! it travels; `invokes` from a binding to the command it invokes.
//!
//! Read when asked, on the host's thread, from the same numbers as the metrics ([`super::metrics`])
//! and the request log: a `routes` edge counts the requests of its route by status class from the
//! [`Recorder`], with the sequences of its latest five records still in the ring; an `asks` edge
//! the answers to its cause by source, from the recorder; an `emits` edge the outcomes of its
//! command that emit the event, from the host's counters, so the edges into one event add up to
//! `spindle_events_appended_total`; a `carries` edge the envelopes its binding sent; an `invokes`
//! edge the envelopes its binding delivered, or for a periodic binding the ticks fired. A `contains`
//! edge carries nothing. Reading it counts nothing.

use std::collections::BTreeMap;
use std::time::SystemTime;

use ess_compiler::ir::{ResolvedCondition, ResolvedOutcome};
use serde_json::{Map, Value, json};
use spindle_host::Host;
use spindle_telemetry::{Carried, Key, Kind, Timed};

use super::{AnswerSource, Recorder, Traffic};

/// The format of the topology.
const FORMAT: &str = "spindle-topology/1";

/// How many of a route's latest requests its edge names.
const LATEST: usize = 5;

/// The id of the ingress's node.
const INGRESS: &str = "ingress";

/// One route of the surface, as its `routes` edge reads its numbers: the method and template it
/// is counted by, and the command or view it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Routed {
    /// The method, upper case, as the request counter labels it.
    pub(crate) method: String,
    /// The path template.
    pub(crate) path: String,
    /// The qualified command or view.
    pub(crate) operation: String,
    /// `command` or `view`.
    pub(crate) kind: &'static str,
}

impl Routed {
    /// The id of the node it routes to: `command:<name>` or `view:<name>`.
    fn node(&self) -> String {
        format!("{}:{}", self.kind, self.operation)
    }
}

/// What the host's counters hold for the edges, summed over its components.
#[derive(Debug, Default)]
struct Counted {
    /// Each declared outcome selected, by command and outcome.
    outcomes: BTreeMap<(String, String), u64>,
    /// Each envelope of an event binding, by binding and what happened to it.
    carried: BTreeMap<(String, Carried), u64>,
    /// Each tick a periodic binding's timer fired, by binding.
    fired: BTreeMap<String, u64>,
}

impl Counted {
    fn of(host: &Host) -> Self {
        let mut counted = Self::default();
        for component in host.placements().keys() {
            let Some(counters) = host.counters(component) else {
                continue;
            };
            let snapshot = counters.snapshot();
            let keys = counters
                .catalogue()
                .counters
                .iter()
                .filter(|counter| counter.kind == Kind::Counter);
            for (counter, value) in keys.zip(snapshot.counters) {
                let value = value.value;
                match &counter.key {
                    Key::Outcome { command, outcome } => {
                        add(
                            &mut counted.outcomes,
                            (command.clone(), outcome.clone()),
                            value,
                        );
                    }
                    Key::Carried { binding, carried } => {
                        add(&mut counted.carried, (binding.clone(), *carried), value);
                    }
                    Key::Timed {
                        binding,
                        timed: Timed::Fired,
                    } => add(&mut counted.fired, binding.clone(), value),
                    _ => {}
                }
            }
        }
        counted
    }
}

fn add<K: Ord>(map: &mut BTreeMap<K, u64>, key: K, value: u64) {
    let held = map.entry(key).or_default();
    *held = held.saturating_add(value);
}

/// The topology of the run `host` runs behind the routes `recorder` counts, with the numbers
/// read now.
pub(crate) fn topology(recorder: &Recorder, host: &Host) -> Value {
    let ir = host.ir();
    let traffic = recorder.traffic();
    let counted = Counted::of(host);
    let specification = ir.system().to_string();
    let mut graph = Graph::default();

    graph.node(INGRESS, "ingress", INGRESS, None);
    graph.node(
        &specification,
        "specification",
        &specification,
        Some(&specification),
    );
    for domain in ir.domains().keys() {
        graph.edge(
            &specification,
            &format!("domain:{domain}"),
            "contains",
            None,
        );
    }
    for domain in ir.domains().keys() {
        let id = format!("domain:{domain}");
        graph.node(&id, "domain", &domain.to_string(), Some(&specification));
        let declared = ir
            .commands()
            .iter()
            .filter(|(_, command)| command.domain.name() == domain)
            .map(|(name, _)| ("command", name.to_string()))
            .chain(
                ir.views()
                    .iter()
                    .filter(|(_, view)| view.domain.name() == domain)
                    .map(|(name, _)| ("view", name.to_string())),
            )
            .chain(
                ir.events()
                    .iter()
                    .filter(|(_, event)| event.domain.name() == domain)
                    .map(|(name, _)| ("event", name.to_string())),
            )
            .collect::<Vec<_>>();
        for (kind, name) in declared {
            let node = format!("{kind}:{name}");
            graph.node(&node, kind, &name, Some(&specification));
            graph.edge(&id, &node, "contains", None);
        }
        for (name, command) in ir.commands() {
            if command.domain.name() != domain {
                continue;
            }
            for outcome in command.outcomes.iter().filter(|outcome| external(outcome)) {
                let cause = format!("{name}/{}", outcome.name);
                graph.node(
                    &format!("external:{cause}"),
                    "external",
                    &cause,
                    Some(&specification),
                );
            }
        }
    }

    for (name, command) in ir.commands() {
        let command_name = name.to_string();
        for outcome in command.outcomes.iter().filter(|outcome| external(outcome)) {
            graph.edge(
                &format!("command:{name}"),
                &format!("external:{name}/{}", outcome.name),
                "asks",
                Some(asked(&traffic, &command_name, outcome.name.as_str())),
            );
        }
    }

    for (name, command) in ir.commands() {
        let mut emitted: Vec<(String, u64)> = Vec::new();
        for outcome in &command.outcomes {
            let selected = counted
                .outcomes
                .get(&(name.to_string(), outcome.name.to_string()))
                .copied()
                .unwrap_or_default();
            for event in &outcome.emits {
                let event = event.to_string();
                match emitted.iter_mut().find(|(held, _)| *held == event) {
                    Some((_, count)) => *count = count.saturating_add(selected),
                    None => emitted.push((event, selected)),
                }
            }
        }
        for (event, count) in emitted {
            graph.edge(
                &format!("command:{name}"),
                &format!("event:{event}"),
                "emits",
                Some(numbered(count)),
            );
        }
    }

    for (name, binding) in ir.bindings() {
        let id = format!("binding:{name}");
        graph.node(&id, "binding", &name.to_string(), Some(&specification));
        let carried = |carried| {
            counted
                .carried
                .get(&(name.to_string(), carried))
                .copied()
                .unwrap_or_default()
        };
        if let Some(event) = binding.cause.event() {
            graph.edge(
                &format!("event:{event}"),
                &id,
                "carries",
                Some(numbered(carried(Carried::Sent))),
            );
        }
    }
    for (name, binding) in ir.bindings() {
        let invoked = if binding.cause.event().is_some() {
            counted
                .carried
                .get(&(name.to_string(), Carried::Delivered))
                .copied()
                .unwrap_or_default()
        } else {
            counted
                .fired
                .get(&name.to_string())
                .copied()
                .unwrap_or_default()
        };
        graph.edge(
            &format!("binding:{name}"),
            &format!("command:{}", binding.command),
            "invokes",
            Some(numbered(invoked)),
        );
    }

    let mut routes = recorder.routes();
    routes.sort_by_key(|route| graph.position(&route.node()));
    let mut routing = Graph::default();
    for route in &routes {
        let traffic = routed(&traffic, recorder, route);
        routing.edge(INGRESS, &route.node(), "routes", Some(traffic));
    }
    graph.edges.splice(0..0, routing.edges);

    json!({
        "format": FORMAT,
        "at": spindle_telemetry::timestamp(SystemTime::now()),
        "specifications": [specification],
        "nodes": graph.nodes,
        "edges": graph.edges,
    })
}

/// Whether `outcome` is decided by an external cause.
fn external(outcome: &ResolvedOutcome) -> bool {
    matches!(
        outcome.condition,
        ResolvedCondition::External { .. }
            | ResolvedCondition::ExternalWhen { .. }
            | ResolvedCondition::ExternalSubject { .. }
    )
}

/// The nodes and edges, in the order they are added.
#[derive(Debug, Default)]
struct Graph {
    nodes: Vec<Value>,
    edges: Vec<Value>,
}

impl Graph {
    fn node(&mut self, id: &str, kind: &str, name: &str, specification: Option<&str>) {
        let mut node = Map::new();
        node.insert("id".to_owned(), json!(id));
        node.insert("kind".to_owned(), json!(kind));
        node.insert("name".to_owned(), json!(name));
        if let Some(specification) = specification {
            node.insert("specification".to_owned(), json!(specification));
        }
        self.nodes.push(Value::Object(node));
    }

    fn edge(&mut self, from: &str, to: &str, kind: &str, traffic: Option<Value>) {
        let mut edge = Map::new();
        edge.insert("from".to_owned(), json!(from));
        edge.insert("to".to_owned(), json!(to));
        edge.insert("kind".to_owned(), json!(kind));
        if let Some(traffic) = traffic {
            edge.insert("traffic".to_owned(), traffic);
        }
        self.edges.push(Value::Object(edge));
    }

    /// Where the node `id` stands among the nodes; after every one where it is none of them.
    fn position(&self, id: &str) -> usize {
        self.nodes
            .iter()
            .position(|node| node["id"] == id)
            .unwrap_or(usize::MAX)
    }
}

/// The numbers of an edge that counts one thing: `count`, and no status, source or request.
fn numbered(count: u64) -> Value {
    json!({ "count": count, "statuses": [], "sources": [], "latest": [] })
}

/// The numbers of the `routes` edge of `route`: its requests by status class, in class order
/// and each class it answered, and the sequences of its latest requests the ring holds.
fn routed(traffic: &Traffic, recorder: &Recorder, route: &Routed) -> Value {
    let mut classes: BTreeMap<&str, u64> = BTreeMap::new();
    for (series, count) in &traffic.requests {
        let [method, path, operation, kind, _, status_class] = series;
        if *method == route.method
            && *path == route.path
            && *operation == route.operation
            && kind.as_str() == route.kind
        {
            add(&mut classes, status_class.as_str(), *count);
        }
    }
    let count = classes
        .values()
        .fold(0u64, |sum, count| sum.saturating_add(*count));
    let statuses = classes
        .iter()
        .map(|(class, count)| json!({ "status_class": class, "count": count }))
        .collect::<Vec<_>>();
    json!({
        "count": count,
        "statuses": statuses,
        "sources": [],
        "latest": recorder.latest(&route.method, &route.path, LATEST),
    })
}

/// The numbers of the `asks` edge to the external cause of `command`'s `outcome`: its answers by
/// source, in source order and each source that answered it.
fn asked(traffic: &Traffic, command: &str, outcome: &str) -> Value {
    let sources = [
        AnswerSource::World,
        AnswerSource::File,
        AnswerSource::Default,
        AnswerSource::Unanswered,
    ]
    .into_iter()
    .filter_map(|source| {
        let count = traffic
            .externals
            .iter()
            .filter(|([asked, cause, by], _)| {
                asked == command && cause == outcome && by == source.name()
            })
            .fold(0u64, |sum, (_, count)| sum.saturating_add(*count));
        (count > 0).then(|| json!({ "source": source.name(), "count": count }))
    })
    .collect::<Vec<_>>();
    let count = traffic
        .externals
        .iter()
        .filter(|([asked, cause, _], _)| asked == command && cause == outcome)
        .fold(0u64, |sum, (_, count)| sum.saturating_add(*count));
    json!({ "count": count, "statuses": [], "sources": sources, "latest": [] })
}
