//! The routes a generic model console reads, served beside the routes ESS publishes where
//! `ingress.network` is bound to `http-loopback`: the model, the events published so far, and a
//! reset of every store, which may load a world.

use ess_compiler::EssIr;
use ess_http::{Method, Operation};
use serde_json::{Value, json};
use spindle_host::{
    Attempted, Cursor, Failed, History, HistoryCursor, Host, HostError, Page, Published, Reader,
    Replayed, Revision, Undelivered, World,
};

use crate::answer::Answer;
use crate::surface::{Surface, form_decoded};

/// One console route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// `GET /api/model`: the compiled IR and each outcome's subject.
    Model,
    /// `GET /api/events`: every event published so far, in order, or a range of them after a
    /// cursor ([`events_queried`]).
    Events,
    /// `POST /api/reset`: empty every store of the run, then load the world its body carries.
    Reset,
    /// `GET /api/served`: every command and view the surface routes, whether it serves it, and
    /// why not ([`served`]).
    Served,
    /// `GET /api/history`: the committed events of one entity instance, oldest first, paged
    /// ([`history_queried`]).
    History,
    /// `GET /api/dead-letters`: every dead letter the run keeps and no replay took, without a
    /// value of its input ([`dead_letters`]).
    DeadLetters,
    /// `POST /api/dead-letters/replay`: one dead letter run once more ([`replay`]).
    Replay,
}

impl Console {
    /// Every console route.
    pub const ALL: [Self; 7] = [
        Self::Model,
        Self::Events,
        Self::Reset,
        Self::Served,
        Self::History,
        Self::DeadLetters,
        Self::Replay,
    ];

    /// The method it answers.
    pub fn method(self) -> Method {
        match self {
            Self::Model | Self::Events | Self::Served | Self::History | Self::DeadLetters => {
                Method::Get
            }
            Self::Reset | Self::Replay => Method::Post,
        }
    }

    /// Its path.
    pub fn path(self) -> &'static str {
        match self {
            Self::Model => "/api/model",
            Self::Events => "/api/events",
            Self::Reset => "/api/reset",
            Self::Served => "/api/served",
            Self::History => "/api/history",
            Self::DeadLetters => "/api/dead-letters",
            Self::Replay => "/api/dead-letters/replay",
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
/// as `{"component", "command", "event", "payload"}`, in the order published, read from the
/// stores that committed them ([`Host::published`]); `500` naming why where a store cannot be
/// read.
pub fn events(host: &mut Host) -> Answer {
    answered(host.published())
}

/// What `GET /api/events` answers, read beside the host's thread through its `reader`, as
/// [`events`] answers it ([`Reader::published`]).
pub fn events_read(reader: &Reader) -> Answer {
    answered(reader.published())
}

/// How many events a range of `GET /api/events` answers where its query names no `limit`.
pub const EVENTS_LIMIT_DEFAULT: usize = 100;

/// The most events a range of `GET /api/events` answers: a larger `limit` is answered with it.
pub const EVENTS_LIMIT_MOST: usize = 1_000;

/// The query parameter a range of `GET /api/events` starts after.
pub(crate) const AFTER: &str = "after";

/// The query parameter that bounds a range of `GET /api/events`.
pub(crate) const LIMIT: &str = "limit";

/// The value of [`AFTER`] that asks for the cursor after the last event published, and no event
/// (`story:events-head-cursor`): no cursor is this text.
pub(crate) const HEAD: &str = "head";

/// Where a range of `GET /api/events` starts.
enum Start {
    /// After a cursor.
    After(Cursor),
    /// After the last event published: the range answers no event, only the cursor.
    Head,
}

/// What `GET /api/events?<query>` answers on the host's thread. Where `query` (after the `?`,
/// still percent-encoded) names neither `after` nor `limit`, [`events`], as without a query; a
/// parameter it does not read is ignored, as a view's query is read. Otherwise `200` with
/// `{"events": [...], "next": <cursor>}`: the events published after the cursor `after` (from
/// the first without one), at most `limit` ([`EVENTS_LIMIT_DEFAULT`] without one, and at most
/// [`EVENTS_LIMIT_MOST`]: a larger one is answered with it), in the order [`events`] lists
/// them, and the cursor after the last of them ([`Host::published_after`]); with the cursor at
/// the end, no event and the same cursor. `after=head` answers no event and the cursor after the
/// last one published, reading no event ([`Host::published_head`]), whatever `limit` it names. A
/// malformed `after` or `limit`, either given twice, a `limit` of 0 and a cursor of another
/// run's components are refused `400` naming the parameter, never its value; a store that cannot
/// be read `500`.
pub fn events_queried(host: &mut Host, query: &str) -> Answer {
    match ranged(query) {
        Ok(None) => events(host),
        Ok(Some((Start::After(after), limit))) => paged(host.published_after(&after, limit)),
        Ok(Some((Start::Head, _))) => paged(host.published_head().map(headed)),
        Err(why) => Answer::refusal(400, why),
    }
}

/// What `GET /api/events?<query>` answers, read beside the host's thread through its `reader`,
/// as [`events_queried`] answers it ([`Reader::published_after`]).
pub fn events_read_queried(reader: &Reader, query: &str) -> Answer {
    match ranged(query) {
        Ok(None) => events_read(reader),
        Ok(Some((Start::After(after), limit))) => paged(reader.published_after(&after, limit)),
        Ok(Some((Start::Head, _))) => paged(reader.published_head().map(headed)),
        Err(why) => Answer::refusal(400, why),
    }
}

/// The page of no event before `next`, the cursor after the last event published.
fn headed(next: Cursor) -> Page {
    Page {
        events: Vec::new(),
        next,
    }
}

/// The range `query` asks for, where it starts and a limit; `None` where it names neither
/// parameter; the refusal of a malformed one, which never repeats its value.
fn ranged(query: &str) -> Result<Option<(Start, usize)>, String> {
    let mut after = Vec::new();
    let mut limit = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match form_decoded(key).as_deref() {
            Some(AFTER) => after.push(value),
            Some(LIMIT) => limit.push(value),
            _ => {}
        }
    }
    if after.is_empty() && limit.is_empty() {
        return Ok(None);
    }
    let after = match one(AFTER, &after)? {
        None => Start::After(Cursor::start()),
        Some(raw) => form_decoded(raw)
            .ok_or_else(|| "expected percent-encoded UTF-8 text".to_owned())
            .and_then(|text| {
                if text == HEAD {
                    Ok(Start::Head)
                } else {
                    Cursor::parse(&text).map(Start::After)
                }
            })
            .map_err(|why| format!("query.{AFTER}: {why}"))?,
    };
    let limit = match one(LIMIT, &limit)? {
        None => EVENTS_LIMIT_DEFAULT,
        Some(raw) => {
            let malformed = || format!("query.{LIMIT}: expected a whole number of events, from 1");
            let text = form_decoded(raw).ok_or_else(malformed)?;
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed());
            }
            // Only digits: a number too large for any integer is larger than the maximum.
            match text.parse::<usize>() {
                Ok(0) => return Err(malformed()),
                Ok(limit) => limit.min(EVENTS_LIMIT_MOST),
                Err(_) => EVENTS_LIMIT_MOST,
            }
        }
    };
    Ok(Some((after, limit)))
}

/// The one value `values` holds for the parameter `name`, if any; the refusal of several.
fn one<'a>(name: &str, values: &[&'a str]) -> Result<Option<&'a str>, String> {
    match values {
        [] => Ok(None),
        [value] => Ok(Some(value)),
        several => Err(format!(
            "query.{name}: expected one value, found {}",
            several.len()
        )),
    }
}

/// The query parameter naming the entity a history read reads, by qualified name.
pub(crate) const ENTITY: &str = "entity";

/// The query parameter naming the instance a history read reads, by its identity.
pub(crate) const IDENTITY: &str = "identity";

/// What a history read asks for: the instance, where it starts and a limit.
struct Asked {
    entity: String,
    identity: Value,
    after: HistoryCursor,
    limit: usize,
}

/// What `GET /api/history?<query>` answers on the host's thread: `200` with `{"events": [...],
/// "next": <cursor>}`, the events committed by every write that wrote the instance of `entity`
/// (a qualified entity name) with `identity`, oldest first, after the cursor `after` (from the
/// first without one), at most `limit` write by write ([`EVENTS_LIMIT_DEFAULT`] without one, and
/// at most [`EVENTS_LIMIT_MOST`]), and the cursor after the last of them ([`Host::history`]).
/// Each event is `{"position", "at", "command", "outcome", "caller", "cause", "event",
/// "payload"}` ([`revision`]). The identity is read as the entity's identity field is
/// ([`identified`]): `identity=t1` and `identity=%22t1%22` both name `"t1"`; `identity=7` names
/// `"7"` where the identity is a `String` and `7` where it is an `Integer`. A query that leaves
/// out `entity` or `identity`, names no entity of the model, gives a parameter twice, or a
/// malformed `after` or `limit` is refused `400` naming the parameter, never its value; a store
/// that cannot be read `500`.
pub fn history_queried(host: &mut Host, query: &str) -> Answer {
    match history_asked(host.ir(), query) {
        Ok(asked) => {
            historied(host.history(&asked.entity, &asked.identity, &asked.after, asked.limit))
        }
        Err(why) => Answer::refusal(400, why),
    }
}

/// What `GET /api/history?<query>` answers, read beside the host's thread through its `reader`,
/// as [`history_queried`] answers it ([`Reader::history`]).
pub fn history_read_queried(reader: &Reader, query: &str) -> Answer {
    match history_asked(reader.ir(), query) {
        Ok(asked) => {
            historied(reader.history(&asked.entity, &asked.identity, &asked.after, asked.limit))
        }
        Err(why) => Answer::refusal(400, why),
    }
}

/// The history read `query` asks for; the refusal of a malformed one, which never repeats a
/// value.
fn history_asked(ir: &EssIr, query: &str) -> Result<Asked, String> {
    let mut given: [(&str, Vec<&str>); 4] = [
        (ENTITY, Vec::new()),
        (IDENTITY, Vec::new()),
        (AFTER, Vec::new()),
        (LIMIT, Vec::new()),
    ];
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let Some(key) = form_decoded(key) else {
            continue;
        };
        if let Some((_, values)) = given.iter_mut().find(|(name, _)| *name == key) {
            values.push(value);
        }
    }
    let [(_, entity), (_, identity), (_, after), (_, limit)] = given;
    let decoded = |name: &str, raw: &str| {
        form_decoded(raw)
            .ok_or_else(|| format!("query.{name}: expected percent-encoded UTF-8 text"))
    };
    let entity = match one(ENTITY, &entity)? {
        Some(raw) => decoded(ENTITY, raw)?,
        None => {
            return Err(format!(
                "query.{ENTITY}: expected an entity's qualified name"
            ));
        }
    };
    let Some(declared) = ir
        .entities()
        .values()
        .find(|declared| declared.name.to_string() == entity)
    else {
        return Err(format!(
            "query.{ENTITY}: expected an entity the model declares"
        ));
    };
    let identity = match one(IDENTITY, &identity)? {
        Some(raw) => identified(
            ess_gen::http::scalar(ir, &declared.identity.type_ref),
            decoded(IDENTITY, raw)?,
        ),
        None => return Err(format!("query.{IDENTITY}: expected an instance's identity")),
    };
    let after = match one(AFTER, &after)? {
        None => HistoryCursor::start(),
        Some(raw) => decoded(AFTER, raw).and_then(|text| {
            HistoryCursor::parse(&text).map_err(|why| format!("query.{AFTER}: {why}"))
        })?,
    };
    let limit = match one(LIMIT, &limit)? {
        None => EVENTS_LIMIT_DEFAULT,
        Some(raw) => limited(raw)?,
    };
    Ok(Asked {
        entity,
        identity,
        after,
        limit,
    })
}

/// The identity the decoded `text` of [`IDENTITY`] names, read as the entity's identity field
/// is (`flat`, [`ess_gen::http::scalar`]): a quoted JSON string is the text it quotes; otherwise
/// a `String` identity, or a newtype of one, is the text as given, digits included, and any
/// other the JSON value `text` spells (`42` a number), or the text where it spells none.
fn identified(flat: Option<ess_gen::http::Flat>, text: String) -> Value {
    if let Ok(quoted @ Value::String(_)) = serde_json::from_str::<Value>(&text) {
        return quoted;
    }
    match flat {
        Some(ess_gen::http::Flat::String) => Value::String(text),
        _ => serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text)),
    }
}

/// The limit `raw`, a value of [`LIMIT`], names: a whole number from 1, at most
/// [`EVENTS_LIMIT_MOST`]; the refusal of another.
fn limited(raw: &str) -> Result<usize, String> {
    let malformed = || format!("query.{LIMIT}: expected a whole number of events, from 1");
    let text = form_decoded(raw).ok_or_else(malformed)?;
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed());
    }
    // Only digits: a number too large for any integer is larger than the maximum.
    match text.parse::<usize>() {
        Ok(0) => Err(malformed()),
        Ok(limit) => Ok(limit.min(EVENTS_LIMIT_MOST)),
        Err(_) => Ok(EVENTS_LIMIT_MOST),
    }
}

/// `200` with `{"events": [...], "next": <cursor>}` for `history`; `400` for an entity the model
/// does not declare, `500` naming why a store could not be read.
fn historied(history: Result<History, HostError>) -> Answer {
    match history {
        Ok(history) => Answer {
            status: 200,
            body: Some(json!({
                "events": history.events.iter().map(revision).collect::<Vec<_>>(),
                "next": history.next.to_string(),
            })),
            headers: Vec::new(),
        },
        Err(HostError::NoEntity(_)) => Answer::refusal(
            400,
            format!("query.{ENTITY}: expected an entity the model declares"),
        ),
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

/// One event of a history as `GET /api/history` answers it: `position` (its write's place in
/// the store's commit order), `at` (its command's time, RFC 3339), `command` and `outcome` (or
/// `null`), `caller` (`{"actor", "attributes", "request"}`: the caller of the request or seed
/// entry that caused it, for a binding's command that of the command whose event the binding
/// carried; `null` where none is known), `cause` (what caused it, [`spindle_kernel::Provenance`]
/// as stored, or `null`), `event` and `payload`.
fn revision(revision: &Revision) -> Value {
    let caller = revision
        .cause
        .as_ref()
        .and_then(spindle_kernel::Provenance::sender)
        .map_or(Value::Null, |sender| {
            json!({
                "actor": sender.actor,
                "attributes": sender.attributes,
                "request": sender.request,
            })
        });
    json!({
        "position": revision.position,
        "at": revision.at,
        "command": revision.command,
        "outcome": revision.outcome,
        "caller": caller,
        "cause": revision.cause,
        "event": revision.event.name,
        "payload": revision.event.payload,
    })
}

/// `200` with `{"events": [...], "next": <cursor>}` for `page`; `400` for a cursor of another
/// run's components, `500` naming why a store could not be read.
fn paged(page: Result<Page, HostError>) -> Answer {
    match page {
        Ok(page) => {
            let mut answer = listed(&page.events);
            if let Some(Value::Object(body)) = &mut answer.body {
                body.insert("next".to_owned(), Value::String(page.next.to_string()));
            }
            answer
        }
        Err(error @ HostError::Cursor(_)) => {
            Answer::refusal(400, format!("query.{AFTER}: {error}"))
        }
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

fn answered(published: Result<Vec<Published>, HostError>) -> Answer {
    match published {
        Ok(published) => listed(&published),
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

/// `200` with `{"events": [...]}`, each of `published` in order.
fn listed(published: &[Published]) -> Answer {
    let events = published
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
        headers: Vec::new(),
    }
}

/// The body `GET /api/served` answers, `spindle.observe.ServedOperations`:
/// `{"operations": [...]}`, one row per route of `surface` in the order its start log names them
/// ([`Surface::operations`]), each `{"component", "name", "kind", "method", "path", "served"}`,
/// `kind` `command` or `view`, and for one not served `reason`, the text [`Route::not_served`]
/// gives it ([`Route::unserved`]). The surface computes it once, when the console routes are
/// added ([`Surface::with_console`]).
///
/// [`Route::not_served`]: crate::Route::not_served
/// [`Route::unserved`]: crate::Route::unserved
pub fn served(surface: &Surface) -> Value {
    let operations = surface
        .operations()
        .into_iter()
        .map(|route| {
            let mut row = json!({
                "component": route.component,
                "name": route.operation.name().to_string(),
                "kind": match route.operation {
                    Operation::Command(_) => "command",
                    Operation::View(_) => "view",
                },
                "method": route.method.as_str().to_uppercase(),
                "path": route.path,
                "served": route.unserved().is_none(),
            });
            if let (Some(why), Value::Object(row)) = (route.unserved(), &mut row) {
                row.insert("reason".to_owned(), Value::String(why.to_string()));
            }
            row
        })
        .collect::<Vec<_>>();
    json!({"operations": operations})
}

/// Reads the world a `POST /api/reset` body carries, as JSON, against the model the host runs;
/// the reason it is refused otherwise.
pub type Worlds<'a> = &'a dyn Fn(&EssIr, &Value) -> Result<World, String>;

/// Whether a `POST /api/reset` whose body is `body` gives a world: any body but an empty one or one
/// all whitespace, which [`reset`] answers by loading no world. A served run with a seed submits
/// it again only after a reset that gives none.
pub fn carries_world(body: &[u8]) -> bool {
    !body.iter().all(u8::is_ascii_whitespace)
}

/// What `POST /api/reset` answers once every store of the run is empty and the world `body`
/// carries is loaded ([`Host::reset_to`]): `200` with `{"emptied": [<component>, ...]}`.
///
/// A body empty or all whitespace loads no world. Any other body is read as JSON by `worlds`; a
/// body that is not JSON, a run with no `worlds`, and a world `worlds` or the host refuses are
/// refused `400` before any store is emptied. A store that could not be emptied or written is
/// refused `500`, naming why.
pub fn reset(host: &mut Host, body: &[u8], worlds: Option<Worlds<'_>>) -> Answer {
    let world = if carries_world(body) {
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
    } else {
        World::default()
    };
    match host.reset_to(world) {
        Ok(emptied) => Answer {
            status: 200,
            body: Some(json!({"emptied": emptied})),
            headers: Vec::new(),
        },
        Err(error @ HostError::World(_)) => Answer::refusal(400, error.to_string()),
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

/// What `GET /api/dead-letters` answers on the host's thread: `200` with `{"dead_letters": [...]}`,
/// each dead letter the run keeps and no replay took ([`Host::dead_letters`]) as
/// `{"id", "component", "binding", "event", "command", "outcome", "error", "attempts"}`, `event`
/// and `outcome` left out where it has none (`spindle.observe.DeadLetters`); never a value of its
/// input. `500` naming why where a store cannot be read.
pub fn dead_letters(host: &mut Host) -> Answer {
    match host.dead_letters() {
        Ok(letters) => Answer {
            status: 200,
            body: Some(json!({"dead_letters": letters.iter().map(letter).collect::<Vec<_>>()})),
            headers: Vec::new(),
        },
        Err(error) => Answer::refusal(500, error.to_string()),
    }
}

/// One dead letter as `GET /api/dead-letters` lists it: what it was kept with but its input.
fn letter(kept: &Undelivered) -> Value {
    let mut entry = serde_json::Map::new();
    entry.insert("id".to_owned(), json!(kept.id));
    entry.insert("component".to_owned(), json!(kept.component));
    entry.insert("binding".to_owned(), json!(kept.letter.binding));
    if let Some(event) = &kept.letter.event {
        entry.insert("event".to_owned(), json!(event));
    }
    entry.insert("command".to_owned(), json!(kept.letter.command));
    if let Some(outcome) = &kept.letter.outcome {
        entry.insert("outcome".to_owned(), json!(outcome));
    }
    entry.insert("error".to_owned(), json!(kept.letter.error));
    entry.insert("attempts".to_owned(), json!(kept.letter.attempts));
    Value::Object(entry)
}

/// What `POST /api/dead-letters/replay` answers on the host's thread, its body
/// `{"component", "id"}` (`spindle.console.Replay`): the dead letter run once more
/// ([`Host::replayed`]), and `200` with `{"component", "id", "command", "result", "outcome",
/// "dead_letter"}` (`spindle.console.ReplayAnswer`) of the replay's own run: `result` `ran`,
/// `refused` or `failed`, `outcome` the outcome it took where it took one, and `dead_letter` the
/// id a refused or failed replay is kept as again. `400` for a body that names no component and
/// id; `404` where no dead letter with that id waits in that component; `500` where a store
/// cannot be read or written.
/// With the answer, every command a binding invoked on the way that did not execute.
pub fn replay(host: &mut Host, body: &[u8]) -> (Answer, Vec<Failed>) {
    let asked = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("the replay body is not JSON: {error}"))
        .and_then(|value| {
            let component = value.get("component").and_then(Value::as_str);
            let id = value.get("id").and_then(Value::as_u64);
            component
                .zip(id)
                .map(|(component, id)| (component.to_owned(), id))
                .ok_or_else(|| {
                    "the replay body names no dead letter: {\"component\": <name>, \"id\": <id>}"
                        .to_owned()
                })
        });
    let (component, id) = match asked {
        Ok(asked) => asked,
        Err(why) => return (Answer::refusal(400, why), Vec::new()),
    };
    let Replayed { own, report } = match host.replayed(&component, id) {
        Ok(Some(replayed)) => replayed,
        Ok(None) => {
            return (
                Answer::refusal(
                    404,
                    format!("no dead letter {id} of {component} waits for a replay"),
                ),
                Vec::new(),
            );
        }
        Err(error) => return (Answer::refusal(500, error.to_string()), Vec::new()),
    };
    // The replay's own run, as the host took it before anything else settled: an envelope a peer
    // process sent meanwhile may run in the same call, and is not this answer's.
    let (command, result, outcome, dead_letter) = match &own {
        Attempted::Ran(ran) => (&ran.command, "ran", Some(&ran.executed.outcome), None),
        Attempted::Refused(refused) => (
            &refused.command,
            "refused",
            Some(&refused.outcome),
            refused.dead_letter,
        ),
        Attempted::Failed(failed) => (&failed.command, "failed", None, failed.dead_letter),
    };
    let mut answer = serde_json::Map::new();
    answer.insert("component".to_owned(), json!(component));
    answer.insert("id".to_owned(), json!(id));
    answer.insert("command".to_owned(), json!(command));
    answer.insert("result".to_owned(), json!(result));
    if let Some(outcome) = outcome {
        answer.insert("outcome".to_owned(), json!(outcome));
    }
    if let Some(dead_letter) = dead_letter {
        answer.insert("dead_letter".to_owned(), json!(dead_letter));
    }
    (
        Answer {
            status: 200,
            body: Some(Value::Object(answer)),
            headers: Vec::new(),
        },
        report.failed,
    )
}
