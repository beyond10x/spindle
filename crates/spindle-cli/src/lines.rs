//! The lines `run` reads on stdin and what it does beside running each line's command: a line is
//! a command or an arrangement of rows ([`Line`]), a command's input may name a value of an
//! earlier line's answer ([`Answered::resolve`]), and the event bindings the command's events
//! match fire as a served run routes them ([`Bindings`]).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use ess_domain::binding::Failure;
use serde_json::{Map, Value, json};
use spindle_channel::{Command, Route, Router};
use spindle_kernel::{Change, Event, ExecuteError, Executed, External, Mint, Store, Write};
use spindle_spec::EssIr;

use crate::Request;
use crate::calls::Calls;
use crate::decisions::Answers;

/// One line of a stdin run.
pub(crate) enum Line {
    /// `{"command": ...}`: a command to run.
    Command(Request),
    /// `{"arrange": [<row>]}`: rows in the reset world's shape, written before the next line.
    Arrange(Vec<Value>),
}

/// An arrange line: nothing beside its rows.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Arrange {
    arrange: Vec<Value>,
}

impl Line {
    /// Reads `text`: an object with the key `arrange` is an arrangement, anything else a command.
    ///
    /// # Errors
    /// Why `text` is neither: not JSON, or not of either shape.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if value.get("arrange").is_some() {
            return serde_json::from_value::<Arrange>(value)
                .map(|arrange| Self::Arrange(arrange.arrange))
                .map_err(|error| format!("an arrange line: {error}"));
        }
        serde_json::from_str::<Request>(text)
            .map(Self::Command)
            .map_err(|error| error.to_string())
    }
}

/// The answer each line of a stdin run printed, by its place in the input, counted from 1: what a
/// later line's reference reads.
#[derive(Default)]
pub(crate) struct Answered(BTreeMap<usize, Value>);

impl Answered {
    /// Keeps `answer` as line `line`'s.
    pub(crate) fn keep(&mut self, line: usize, answer: Value) {
        self.0.insert(line, answer);
    }

    /// Replaces each reference in `input`, at any depth, by the value it names: an object
    /// `{"$line": <n>, "pointer": "<JSON pointer>"}` is the value at that pointer in the answer
    /// line `n` printed, `n` before `line`.
    ///
    /// # Errors
    /// A reference of another shape, to a line not answered before `line` (a later line, `line`
    /// itself, a blank line or none), or to a pointer that answer lacks; nothing is replaced.
    pub(crate) fn resolve(
        &self,
        line: usize,
        input: &mut Map<String, Value>,
    ) -> Result<(), String> {
        let mut resolved = input.clone();
        for value in resolved.values_mut() {
            self.replace(line, value)?;
        }
        *input = resolved;
        Ok(())
    }

    fn replace(&self, line: usize, value: &mut Value) -> Result<(), String> {
        match value {
            Value::Object(object) if object.contains_key("$line") => {
                *value = self.referenced(line, object)?;
            }
            Value::Object(object) => {
                for member in object.values_mut() {
                    self.replace(line, member)?;
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.replace(line, item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn referenced(&self, line: usize, reference: &Map<String, Value>) -> Result<Value, String> {
        let target = reference
            .get("$line")
            .and_then(Value::as_u64)
            .and_then(|target| usize::try_from(target).ok());
        let pointer = reference.get("pointer").and_then(Value::as_str);
        let (Some(target), Some(pointer), 2) = (target, pointer, reference.len()) else {
            return Err(format!(
                "{} is no reference: one is {{\"$line\": <line number>, \"pointer\": \"<JSON \
                 pointer>\"}} and nothing else",
                Value::Object(reference.clone())
            ));
        };
        if target >= line {
            return Err(format!(
                "line {line} names line {target}, which is not answered before it"
            ));
        }
        let answer = self
            .0
            .get(&target)
            .ok_or_else(|| format!("line {line} names line {target}, which is blank or no line"))?;
        answer.pointer(pointer).cloned().ok_or_else(|| {
            format!(
                "line {line} names {pointer} of line {target}'s answer, which has none: {answer}"
            )
        })
    }
}

/// One row an arrange line wrote.
pub(crate) struct Arranged {
    /// The entity, by qualified name.
    pub entity: String,
    /// The identity, as the store holds it.
    pub identity: Value,
}

impl Arranged {
    /// The result line of an arrange line that wrote `rows`: `{"arranged": [{"entity",
    /// "identity"}]}`, in the line's order.
    pub(crate) fn answer(rows: &[Self]) -> Value {
        let rows = rows
            .iter()
            .map(|row| json!({"entity": row.entity, "identity": row.identity}))
            .collect::<Vec<_>>();
        json!({ "arranged": rows })
    }
}

/// Writes `rows`, in the reset world's shape, to `store`: each checked by ESS as a scenario's setup
/// is and by the host's reader ([`spindle_host::read_world`]), then against its entity's invariants
/// ([`spindle_kernel::at_rest`]), then written all together or not at all
/// ([`Store::commit_all`]).
///
/// # Errors
/// The first row refused, by its place in the line counted from 0, or one already stored or named
/// twice; nothing is written.
pub(crate) fn arrange(
    ir: &EssIr,
    store: &mut dyn Store,
    rows: Vec<Value>,
) -> Result<Vec<Arranged>, String> {
    let world = spindle_host::read_world(ir, &json!({"rows": rows}), |_| {
        Box::new(spindle_kernel::NoExternals)
    })
    .map_err(|why| why.replace("reading the reset's world: ", ""))?;
    let mut named = BTreeSet::new();
    for (index, seed) in world.rows.iter().enumerate() {
        let (entity, identity) = (&seed.entity, &seed.identity);
        spindle_kernel::at_rest(ir, entity, identity, &seed.row)
            .map_err(|error| format!("row {index}: {entity} {identity}: {error}"))?;
        if !named.insert((entity.clone(), identity.to_string())) {
            return Err(format!(
                "row {index}: {entity} {identity} is already a row of this line"
            ));
        }
        let stored = store
            .load(entity, identity)
            .map_err(|error| format!("row {index}: reading {entity} {identity}: {error}"))?;
        if stored.is_some() {
            return Err(format!(
                "row {index}: {entity} {identity} is already stored, and an arrange line never \
                 replaces an instance"
            ));
        }
    }
    let writes = world
        .rows
        .iter()
        .map(|seed| {
            Write::Commit(Change {
                entity: seed.entity.clone(),
                identity: seed.identity.clone(),
                expected: None,
                row: seed.row.clone(),
                events: Vec::new(),
                scanned: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    store
        .commit_all(&writes)
        .map_err(|error| format!("writing the rows: {error}"))?;
    Ok(world
        .rows
        .into_iter()
        .map(|seed| Arranged {
            entity: seed.entity,
            identity: seed.identity,
        })
        .collect())
}

/// What one command of a stdin run runs on: the run's model, its one store, its answers to external
/// causes, its mint and its clock.
pub(crate) struct Running<'a> {
    pub ir: &'a EssIr,
    pub store: &'a mut dyn Store,
    pub answers: &'a Answers,
    pub calls: &'a Calls,
    pub mint: &'a mut dyn Mint,
    pub now: &'a dyn Fn() -> SystemTime,
}

impl Running<'_> {
    /// Runs `invocation` once, reaching every component, whose rows the one store holds
    /// (`story:cross-component-related-reads`): a cause bound to an HTTP call is answered by it,
    /// any other by `external`, then as the run's answers answer it.
    pub(crate) fn execute(
        &mut self,
        invocation: &spindle_kernel::Invocation<'_>,
        external: &BTreeMap<String, bool>,
    ) -> Result<Executed, ExecuteError> {
        let (calls, answers) = (self.calls, self.answers);
        let mut decider = |question: &External<'_>| {
            calls
                .answer(question)
                .unwrap_or_else(|| answers.decide(external, question))
        };
        spindle_kernel::execute_reaching(
            self.ir,
            self.store,
            &mut decider,
            self.mint,
            self.now,
            invocation,
            &BTreeMap::new(),
            &crate::every_component(self.ir),
        )
    }
}

/// The result line of `command`: its outcome with its events, declared error and response, or the
/// failure the kernel answered.
pub(crate) fn printed(command: &str, result: &Result<Executed, ExecuteError>) -> Value {
    match result {
        Ok(executed) => {
            let mut printed = json!({"command": command});
            if let (Some(object), Ok(Value::Object(fields))) =
                (printed.as_object_mut(), serde_json::to_value(executed))
            {
                object.extend(fields);
            }
            printed
        }
        Err(error) => json!({
            "command": command,
            "failure": {"kind": error.kind(), "message": error.to_string()},
        }),
    }
}

/// The event bindings of a stdin run, routed by the router a served run routes them with.
pub(crate) struct Bindings {
    router: Router,
}

/// One command a binding invoked: its result line, and why it fails the run where it does.
pub(crate) struct Fired {
    /// The result line, with `caused_by` and `attempts`.
    pub line: Value,
    /// What stderr says of it where it was refused or did not run.
    pub rejected: Option<String>,
}

impl Bindings {
    /// The bindings of `ir`, read from `spec`: routed as a served run routes them, a specification
    /// that declares no component as the one implied component
    /// ([`spindle_spec::load_with_implied_component`]). With `drop_retries`, each binding that
    /// retries a failure drops it instead, and one line per such binding is printed on stderr.
    ///
    /// # Errors
    /// A binding the router refuses, named, as a served run refuses it before listening.
    pub(crate) fn new(spec: &Path, ir: &EssIr, drop_retries: bool) -> Result<Self> {
        let evented = ir
            .bindings()
            .values()
            .any(|binding| binding.cause.event().is_some());
        let implied;
        let routed = if evented && ir.components().is_empty() {
            implied = spindle_spec::load_with_implied_component(spec)?;
            &implied.ir
        } else {
            ir
        };
        let router = Router::from_ir(routed)
            .with_context(|| format!("routing the bindings of {}", spec.display()))?;
        let router = if drop_retries {
            for binding in spindle_host::retried(ir) {
                eprintln!(
                    "b10x-spindle: binding {binding} retries on failure; --drop-retries: a failed \
                     invocation is dropped, not retried"
                );
            }
            router.dropping_retries()
        } else {
            router
        };
        Ok(Self { router })
    }

    /// Whether the specification declares no event binding: its runs print as before bindings
    /// fired on stdin.
    pub(crate) fn is_empty(&self) -> bool {
        self.router.routes().is_empty()
    }

    /// Runs every command the bindings invoke for `events`, emitted by the command of stdin line
    /// `line`, and for the events those commands emit in turn, in the order a served run delivers
    /// them (`spindle_host::Host`'s `settle`): one link per publishing and accepting component,
    /// the links in name order, each drained of what it carries, what those commands send it
    /// included, before the next, round after round until none carries anything. Each command is
    /// sent as no actor, its external causes answered by the run's answers. A command that is
    /// refused or does not run is attempted again where its binding retries it, up to
    /// [`spindle_host::RETRY_ATTEMPTS`] in all. Stops after [`spindle_host::DELIVERY_LIMIT`]
    /// commands, the rest rejected as a runaway.
    pub(crate) fn fire(
        &self,
        running: &mut Running<'_>,
        line: usize,
        events: Vec<Event>,
    ) -> Vec<Fired> {
        let mut fired = Vec::new();
        let mut links: BTreeMap<(String, String), VecDeque<(&Route, Command)>> = self
            .router
            .routes()
            .iter()
            .map(|route| ((route.from.clone(), route.to.clone()), VecDeque::new()))
            .collect();
        let order: Vec<(String, String)> = links.keys().cloned().collect();
        self.publish(line, events, &mut links, &mut fired);
        let mut deliveries = 0_usize;
        loop {
            let mut delivered = false;
            for link in &order {
                while let Some((route, command)) = links.get_mut(link).and_then(VecDeque::pop_front)
                {
                    delivered = true;
                    let caused_by = json!({"line": line, "binding": route.binding});
                    if deliveries >= spindle_host::DELIVERY_LIMIT {
                        fired.push(Fired {
                            line: json!({
                                "command": route.command,
                                "failure": {
                                    "kind": "runaway",
                                    "message": format!(
                                        "stopped after {deliveries} commands the bindings of one \
                                         line invoked"
                                    ),
                                },
                                "caused_by": caused_by,
                                "attempts": 0,
                            }),
                            rejected: Some(format!(
                                "line {line}: its bindings kept invoking commands; stopped after \
                                 {deliveries}, binding {} the last",
                                route.binding
                            )),
                        });
                        return fired;
                    }
                    deliveries += 1;
                    let (attempts, result) =
                        attempted(running, route, &command.name, &command.fields);
                    let mut printed = printed(&command.name, &result);
                    if let Some(object) = printed.as_object_mut() {
                        object.insert("caused_by".to_owned(), caused_by);
                        object.insert("attempts".to_owned(), json!(attempts));
                    }
                    let rejected = match &result {
                        Ok(Executed { error: None, .. }) => None,
                        Ok(Executed { outcome, .. }) => Some(format!("was refused ({outcome})")),
                        Err(error) => Some(format!("did not run ({})", error.kind())),
                    }
                    .map(|what| {
                        format!(
                            "line {line}: binding {} invoked {}, which {what} after {attempts} \
                             attempt(s)",
                            route.binding, command.name
                        )
                    });
                    fired.push(Fired {
                        line: printed,
                        rejected,
                    });
                    if let Ok(Executed {
                        error: None,
                        events,
                        ..
                    }) = result
                    {
                        self.publish(line, events, &mut links, &mut fired);
                    }
                }
            }
            if !delivered {
                return fired;
            }
        }
    }

    /// Puts the command each binding maps each of `events` to on its link, in the order the
    /// events were emitted and, per event, in binding-name order, as a served run's sender stages
    /// them; a binding whose input is unmet or whose condition is undecided is printed as a
    /// command that did not run.
    fn publish<'r>(
        &'r self,
        line: usize,
        events: Vec<Event>,
        links: &mut BTreeMap<(String, String), VecDeque<(&'r Route, Command)>>,
        fired: &mut Vec<Fired>,
    ) {
        for event in events {
            let event = spindle_channel::Event {
                name: event.name,
                fields: event.payload,
            };
            for route in self
                .router
                .routes()
                .iter()
                .filter(|route| route.event == event.name)
            {
                match route.invoke(&event) {
                    Ok(Some(command)) => {
                        if let Some(link) = links.get_mut(&(route.from.clone(), route.to.clone())) {
                            link.push_back((route, command));
                        }
                    }
                    Ok(None) => {}
                    Err(unmet) => fired.push(Fired {
                        line: json!({
                            "command": route.command,
                            "failure": {"kind": "unmet_binding", "message": unmet.to_string()},
                            "caused_by": {"line": line, "binding": route.binding},
                            "attempts": 0,
                        }),
                        rejected: Some(format!(
                            "line {line}: binding {} did not invoke {}: {unmet}",
                            route.binding, route.command
                        )),
                    }),
                }
            }
        }
    }
}

/// Runs `command` as `route` invoked it, again after each attempt the route answers with
/// [`Failure::Retry`] ([`Route::answer`]), up to [`spindle_host::RETRY_ATTEMPTS`] in all, as a
/// served run attempts it; answers how many attempts were made and the last one's result.
fn attempted(
    running: &mut Running<'_>,
    route: &Route,
    command: &str,
    input: &Map<String, Value>,
) -> (u32, Result<Executed, ExecuteError>) {
    let invocation = spindle_kernel::Invocation {
        command,
        input,
        actor: None,
        caller: None,
    };
    let mut attempts = 0;
    loop {
        attempts += 1;
        let result = running.execute(&invocation, &BTreeMap::new());
        let refusal = match &result {
            Ok(Executed { error: None, .. }) => None,
            Ok(Executed { outcome, .. }) => Some(Some(outcome.as_str())),
            Err(_) => Some(None),
        };
        let again = refusal.is_some_and(|refusal| route.answer(refusal) == Failure::Retry);
        if again && attempts < spindle_host::RETRY_ATTEMPTS {
            continue;
        }
        return (attempts, result);
    }
}
