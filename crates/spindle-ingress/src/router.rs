//! The routing of a request to the command, view or console route it asks for and its answer on a
//! [`Host`]: a function from a [`Request`] value to a [`Response`] value, with no listener,
//! socket or async runtime in it. The loopback listener calls it for every request it reads, and a
//! host with no socket, such as a Service Worker, calls [`route`] itself.

use serde_json::{Map, Value};
use spindle_host::{Failed, Host};

use crate::answer::Answer;
use crate::console::{Console, Worlds};
use crate::surface::{Resolved, Route, Surface};

/// The largest request body read, in bytes: [`route`] refuses a larger one `413`, and the
/// listener one announced larger before it is read.
pub const BODY_LIMIT: usize = 1 << 20;

/// The media type of every [`Response`] body.
pub const CONTENT_TYPE: &str = "application/json";

/// One request, as a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method, compared exactly (RFC 9110 §9.1): `GET`, `POST`, ...
    pub method: String,
    /// The path, without the query string, still percent-encoded.
    pub path: String,
    /// The query string, after the `?` and still percent-encoded; empty without one.
    pub query: String,
    /// The `Content-Type`, where the request has one.
    pub content_type: Option<String>,
    /// The body.
    pub body: Vec<u8>,
}

/// One answer, as a value: its body is [`CONTENT_TYPE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status.
    pub status: u16,
    /// The methods the path answers, as the `Allow` header carries them, on a `405`.
    pub allow: Option<String>,
    /// The body.
    pub body: String,
}

impl From<Answer> for Response {
    fn from(answer: Answer) -> Self {
        Self {
            status: answer.status,
            allow: None,
            body: answer.body.to_string(),
        }
    }
}

/// A [`Response`], and every command a binding invoked on the way that did not execute
/// ([`crate::answered`]).
#[derive(Debug)]
pub struct Routed {
    /// The answer.
    pub response: Response,
    /// The commands that were dropped; none runs again.
    pub failed: Vec<Failed>,
}

/// Answers `request` on `host` at the routes of `surface`: a command or view as
/// [`answer`](crate::answer) answers it, a console route as [`crate::console`] does. A reset that
/// carries a world is refused; [`route_with`] loads one.
///
/// Each refusal is `{"refused": <why>}`, as ESS's served application writes it: a path no route
/// has `404`; a served path under another method `405` with [`Response::allow`], the method
/// compared exactly; a path variable that does not decode `400`; a body over [`BODY_LIMIT`]
/// `413`.
pub fn route(surface: &Surface, host: &mut Host, request: Request) -> Routed {
    routing(surface, host, request, None)
}

/// [`route`], a reset's world read by `worlds` ([`crate::console::reset`]).
pub fn route_with(
    surface: &Surface,
    host: &mut Host,
    request: Request,
    worlds: Worlds<'_>,
) -> Routed {
    routing(surface, host, request, Some(worlds))
}

fn routing(
    surface: &Surface,
    host: &mut Host,
    request: Request,
    worlds: Option<Worlds<'_>>,
) -> Routed {
    let (asked, carried) = match ask(surface, &request.method, &request.path) {
        Ok(asked) => asked,
        Err(response) => {
            return Routed {
                response,
                failed: Vec::new(),
            };
        }
    };
    if request.body.len() > BODY_LIMIT {
        return Routed {
            response: oversized().into(),
            failed: Vec::new(),
        };
    }
    let (response, failed) = Pending {
        asked,
        carried,
        query: request.query,
        content_type: request.content_type,
        body: request.body,
    }
    .serve(host, worlds);
    Routed { response, failed }
}

/// The refusal of a body over [`BODY_LIMIT`], `413`: the one text [`route`] and the listener
/// answer, true of a body the request carries and of one its `Content-Length` announces.
pub(crate) fn oversized() -> Answer {
    Answer::refusal(413, format!("the body is larger than {BODY_LIMIT} bytes"))
}

/// What a request asks for.
#[derive(Debug)]
pub(crate) enum Asked {
    /// A command or view the surface serves.
    Route(Box<Route>),
    /// A console route.
    Console(Console),
}

/// What `method` and `path` ask for on `surface`, and the input the path carries; the refusal
/// otherwise: `400` for a path variable that does not decode, `405` with `Allow` for a served path
/// under another method, `404` for a path no route has.
pub(crate) fn ask(
    surface: &Surface,
    method: &str,
    path: &str,
) -> Result<(Asked, Map<String, Value>), Response> {
    match surface.resolve(method, path) {
        Resolved::Found(route, carried) => Ok((Asked::Route(Box::new(route.clone())), carried)),
        Resolved::Console(console) => Ok((Asked::Console(console), Map::new())),
        Resolved::Malformed(why) => Err(Answer::refusal(400, why).into()),
        Resolved::WrongMethod(allowed) => {
            let allowed = allowed
                .iter()
                .map(|method| method.as_str().to_uppercase())
                .collect::<Vec<_>>()
                .join(", ");
            let mut response: Response =
                Answer::refusal(405, format!("this path answers {allowed}")).into();
            response.allow = Some(allowed);
            Err(response)
        }
        Resolved::NotFound => Err(Answer::refusal(404, format!("no route serves {path}")).into()),
    }
}

/// A request whose route is resolved and whose body is read, waiting for a host to answer it.
#[derive(Debug)]
pub(crate) struct Pending {
    pub(crate) asked: Asked,
    pub(crate) carried: Map<String, Value>,
    pub(crate) query: String,
    pub(crate) content_type: Option<String>,
    pub(crate) body: Vec<u8>,
}

impl Pending {
    /// Answers the request on `host`, a reset's world read by `worlds` where given.
    pub(crate) fn serve(
        self,
        host: &mut Host,
        worlds: Option<Worlds<'_>>,
    ) -> (Response, Vec<Failed>) {
        match &self.asked {
            Asked::Route(route) => {
                let (answer, failed) = crate::answered(
                    host,
                    route,
                    &self.carried,
                    &self.query,
                    self.content_type.as_deref(),
                    &self.body,
                );
                (answer.into(), failed)
            }
            Asked::Console(Console::Model) => (
                Response {
                    status: 200,
                    allow: None,
                    body: crate::console::model(host.ir()),
                },
                Vec::new(),
            ),
            Asked::Console(Console::Events) => (crate::console::events(host).into(), Vec::new()),
            Asked::Console(Console::Reset) => (
                crate::console::reset(host, &self.body, worlds).into(),
                Vec::new(),
            ),
        }
    }
}
