//! The routes a generic model console reads, served beside the routes ESS publishes where
//! `ingress.network` is bound to `http-loopback`: the model, the events published so far, and a
//! reset of every store, which may load a world.

use ess_compiler::EssIr;
use ess_http::Method;
use serde_json::{Value, json};
use spindle_host::{Host, HostError, World};

use crate::answer::Answer;

/// One console route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// `GET /api/model`: the compiled IR and each outcome's subject.
    Model,
    /// `GET /api/events`: every event published so far, in order.
    Events,
    /// `POST /api/reset`: empty every store of the run, then load the world its body carries.
    Reset,
}

impl Console {
    /// Every console route.
    pub const ALL: [Self; 3] = [Self::Model, Self::Events, Self::Reset];

    /// The method it answers.
    pub fn method(self) -> Method {
        match self {
            Self::Model | Self::Events => Method::Get,
            Self::Reset => Method::Post,
        }
    }

    /// Its path.
    pub fn path(self) -> &'static str {
        match self {
            Self::Model => "/api/model",
            Self::Events => "/api/events",
            Self::Reset => "/api/reset",
        }
    }
}

impl std::fmt::Display for Console {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} {}",
            self.method().as_str().to_uppercase(),
            self.path()
        )
    }
}

/// The body `GET /api/model` answers, as text: `{"ir": <IR>, "subjects": [...]}`.
///
/// `ir` is written byte for byte as `ess specify compile --format json` prints it
/// ([`EssIr::to_canonical_json`]). `subjects` holds one entry per outcome of every command, in
/// the IR's order: `{"command", "outcome", "subject"}`, `subject` the entity the outcome acts on,
/// what it does to it and where the instance's identity is read, as the IR carries it, or `null`
/// for an outcome that changes no entity.
pub fn model(ir: &EssIr) -> String {
    let subjects = ir
        .commands()
        .iter()
        .flat_map(|(name, command)| {
            command.outcomes.iter().map(move |outcome| {
                json!({
                    "command": name.to_string(),
                    "outcome": outcome.name.to_string(),
                    "subject": serde_json::to_value(&outcome.subject).unwrap_or(Value::Null),
                })
            })
        })
        .collect::<Vec<_>>();
    format!(
        "{{\"ir\":{},\"subjects\":{}}}",
        ir.to_canonical_json(),
        Value::Array(subjects)
    )
}

/// What `GET /api/events` answers: `200` with `{"events": [...]}`, each event the host published
/// as `{"component", "command", "event", "payload"}`, in the order published.
pub fn events(host: &Host) -> Answer {
    let events = host
        .published()
        .iter()
        .map(|published| {
            json!({
                "component": published.component,
                "command": published.command,
                "event": published.event.name,
                "payload": published.event.payload,
            })
        })
        .collect::<Vec<_>>();
    Answer {
        status: 200,
        body: Some(json!({"events": events})),
    }
}

/// Reads the world a `POST /api/reset` body carries, as JSON, against the model the host runs;
/// the reason it is refused otherwise.
pub type Worlds<'a> = &'a dyn Fn(&EssIr, &Value) -> Result<World, String>;

/// What `POST /api/reset` answers once every store of the run is empty and the world `body`
/// carries is loaded ([`Host::reset_to`]): `200` with `{"emptied": [<component>, ...]}`.
///
/// A body empty or all whitespace loads no world. Any other body is read as JSON by `worlds`; a
/// body that is not JSON, a run with no `worlds`, and a world `worlds` or the host refuses are
/// refused `400` before any store is emptied. A store that could not be emptied or written is
/// refused `500`, naming why.
pub fn reset(host: &mut Host, body: &[u8], worlds: Option<Worlds<'_>>) -> Answer {
    let world = if body.iter().all(u8::is_ascii_whitespace) {
        World::default()
    } else {
        let read = serde_json::from_slice::<Value>(body)
            .map_err(|error| format!("the reset body is not JSON: {error}"))
            .and_then(|value| match worlds {
                Some(worlds) => worlds(host.ir(), &value),
                None => Err("this run loads no world; send the reset without a body".to_owned()),
            });
        match read {
            Ok(world) => world,
            Err(why) => return Answer::refusal(400, why),
        }
    };
    match host.reset_to(world) {
        Ok(emptied) => Answer {
            status: 200,
            body: Some(json!({"emptied": emptied})),
        },
        Err(error @ HostError::World(_)) => Answer::refusal(400, error.to_string()),
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}
