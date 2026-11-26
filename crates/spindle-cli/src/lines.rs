//! What `run` reading stdin does beside running each line's command: it fires the event bindings
//! the command's events match, as a served run routes them ([`Bindings`]).

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use ess_domain::binding::Failure;
use serde_json::{Map, Value, json};
use spindle_channel::{Route, Router};
use spindle_kernel::{Event, ExecuteError, Executed, External, Mint, Store};
use spindle_spec::EssIr;

use crate::calls::Calls;
use crate::decisions::Answers;

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
    /// Runs `invocation` once: a cause bound to an HTTP call is answered by it, any other by
    /// `external`, then as the run's answers answer it.
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
        spindle_kernel::execute_as(
            self.ir,
            self.store,
            &mut decider,
            self.mint,
            self.now,
            invocation,
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

    /// Runs every command the bindings invoke for `events`, emitted by the command of stdin line
    /// `line`, and for the events those commands emit in turn, in the order they were emitted:
    /// each sent as no actor, its external causes answered by the run's answers. A command that
    /// is refused or does not run is attempted again where its binding retries it, up to
    /// [`spindle_host::RETRY_ATTEMPTS`] in all. Stops after [`spindle_host::DELIVERY_LIMIT`]
    /// commands, the rest rejected as a runaway.
    pub(crate) fn fire(
        &self,
        running: &mut Running<'_>,
        line: usize,
        events: Vec<Event>,
    ) -> Vec<Fired> {
        let mut fired = Vec::new();
        let mut queue = VecDeque::from(events);
        let mut deliveries = 0_usize;
        while let Some(event) = queue.pop_front() {
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
                let caused_by = json!({"line": line, "binding": route.binding});
                let command = match route.invoke(&event) {
                    Ok(Some(command)) => command,
                    Ok(None) => continue,
                    Err(unmet) => {
                        fired.push(Fired {
                            line: json!({
                                "command": route.command,
                                "failure": {"kind": "unmet_binding", "message": unmet.to_string()},
                                "caused_by": caused_by,
                                "attempts": 0,
                            }),
                            rejected: Some(format!(
                                "line {line}: binding {} did not invoke {}: {unmet}",
                                route.binding, route.command
                            )),
                        });
                        continue;
                    }
                };
                if deliveries >= spindle_host::DELIVERY_LIMIT {
                    fired.push(Fired {
                        line: json!({
                            "command": route.command,
                            "failure": {
                                "kind": "runaway",
                                "message": format!(
                                    "stopped after {deliveries} commands the bindings of one line invoked"
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
                let (attempts, result) = attempted(running, route, &command.name, &command.fields);
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
                if let Ok(Executed {
                    error: None,
                    events,
                    ..
                }) = result
                {
                    queue.extend(events);
                }
                fired.push(Fired {
                    line: printed,
                    rejected,
                });
            }
        }
        fired
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
