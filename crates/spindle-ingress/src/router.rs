//! The routing of a request to the command, view or console route it asks for and its answer on a
//! [`Host`]: a function from a [`Request`] value to a [`Response`] value, with no listener,
//! socket or async runtime in it. The loopback listener calls it for every request it reads, and a
//! host with no socket, such as a Service Worker, calls [`route`] itself.

use std::sync::Arc;

use ess_http::Operation;
use serde_json::{Map, Value};
use spindle_host::{Failed, Host, Reader};

use crate::answer::{Answer, Carried};
use crate::console::{Console, Worlds};
use crate::observe::{Answered, By, Recorder, Reserved, UnmetInput};
use crate::surface::{Caller, Resolved, Route, Surface};

/// The largest request body read, in bytes: [`route`] refuses a larger one `413`, and the
/// listener one announced larger before it is read.
pub const BODY_LIMIT: usize = 1 << 20;

/// The media type of every [`Response`] body that is not empty.
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

/// One answer, as a value: its body, where it is not empty, is [`CONTENT_TYPE`], the observe
/// panel's page where [`Response::panel`] says so, or the metrics where [`Response::metrics`]
/// does; an empty body carries no `Content-Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status.
    pub status: u16,
    /// The methods the path answers, as the `Allow` header carries them, on a `405`.
    pub allow: Option<String>,
    /// The body is the observe panel's page ([`crate::Surface::with_panel`]): it is
    /// [`PANEL_MEDIA`](crate::PANEL_MEDIA), answered with
    /// [`PANEL_POLICY`](crate::PANEL_POLICY) as its `Content-Security-Policy`.
    pub panel: bool,
    /// The body is the metrics, `GET /__spindle/metrics`: it is
    /// [`METRICS_MEDIA`](crate::METRICS_MEDIA), the Prometheus text format.
    pub metrics: bool,
    /// The response headers the binding declares for the outcome answered ([`Answer::headers`]),
    /// as `(name, value)` in name order; a header the transport writes itself is never replaced by
    /// one of these.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: String,
}

impl From<Answer> for Response {
    fn from(answer: Answer) -> Self {
        Self {
            status: answer.status,
            allow: None,
            panel: false,
            metrics: false,
            headers: answer.headers,
            body: answer.body.map(|body| body.to_string()).unwrap_or_default(),
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
/// `413`. A [`Request`] carries no header, so on a surface made [`Surface::proving_callers`] it
/// proves no caller and carries no proxy secret: every command, view, console route and route
/// under `/__spindle/` is refused `403` `{"refused": "not granted", "actor": null}`.
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
    // A request value carries no header, so on a surface whose requests prove their caller it
    // carries neither a caller nor the proxy's secret, which the run's own routes answer alone.
    let proving = surface.caller_header().is_some();
    let unproven = || Routed {
        response: Answer::not_proven().into(),
        failed: Vec::new(),
    };
    if proving && Reserved::of(&request.path).is_some() {
        return unproven();
    }
    if let Some(reserved) = Reserved::of(&request.path) {
        let answered = crate::observe::reserved(
            surface.recorder(),
            surface.panel(),
            reserved,
            &request.method,
            &request.path,
            &request.query,
        );
        let response = match (answered, surface.recorder()) {
            (Some(response), _) => response,
            (None, Some(recorder)) => crate::observe::hosted(reserved, recorder, host),
            (None, None) => {
                Answer::refusal(404, format!("no route serves {}", request.path)).into()
            }
        };
        return Routed {
            response,
            failed: Vec::new(),
        };
    }
    let (asked, carried) = match ask(surface, &request.method, &request.path) {
        Ok(asked) => asked,
        Err((response, ..)) => {
            return Routed {
                response,
                failed: Vec::new(),
            };
        }
    };
    if proving && matches!(asked, Asked::Console(_)) {
        return unproven();
    }
    if request.body.len() > BODY_LIMIT {
        return Routed {
            response: oversized().into(),
            failed: Vec::new(),
        };
    }
    let (response, failed, _) = Pending {
        asked,
        carried,
        path: request.path,
        query: request.query,
        content_type: request.content_type,
        body: request.body,
        recorder: surface.recorder().cloned(),
        served: surface.served(),
        proven: None,
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
    /// `GET /__spindle/metrics` or `GET /__spindle/topology`, on an observing surface, handed to
    /// the host's thread by the listener ([`crate::observe::hosted`]); [`route`] answers it
    /// directly.
    #[cfg_attr(not(feature = "listener"), allow(dead_code))]
    Hosted(Reserved),
}

/// A refusal, who gave it, and the route it was for where the path matched one.
pub(crate) type Refusal = (Response, By, Option<Asked>);

/// What `method` and `path` ask for on `surface`, and the input the path carries; the refusal
/// otherwise: `400` for a path variable that does not decode, `405` with `Allow` for a served path
/// under another method, `404` for a path no route has or one under the reserved prefix (which
/// [`route`] and the listener answer before asking), each with who refused it.
pub(crate) fn ask(
    surface: &Surface,
    method: &str,
    path: &str,
) -> Result<(Asked, Map<String, Value>), Refusal> {
    let not_found = || Answer::refusal(404, format!("no route serves {path}")).into();
    match surface.resolve(method, path) {
        Resolved::Found(route, carried) => Ok((Asked::Route(Box::new(route.clone())), carried)),
        Resolved::Console(console) => Ok((Asked::Console(console), Map::new())),
        Resolved::Malformed(route, why) => Err((
            Answer::refusal(400, why).into(),
            By::Ingress,
            Some(Asked::Route(Box::new(route.clone()))),
        )),
        Resolved::WrongMethod(allowed) => {
            let allowed = allowed
                .iter()
                .map(|method| method.as_str().to_uppercase())
                .collect::<Vec<_>>()
                .join(", ");
            let mut response: Response =
                Answer::refusal(405, format!("this path answers {allowed}")).into();
            response.allow = Some(allowed);
            Err((response, By::Unrouted, None))
        }
        Resolved::NotFound => Err((not_found(), By::Unrouted, None)),
        Resolved::Reserved(_) => Err((not_found(), By::Unrouted, None)),
    }
}

/// A request whose route is resolved and whose body is read, waiting for a host to answer it.
#[derive(Debug)]
pub(crate) struct Pending {
    pub(crate) asked: Asked,
    pub(crate) carried: Map<String, Value>,
    /// The request's path, still percent-encoded: on a route whose requests prove their caller
    /// ([`Route::proves`]) the router leaves `carried` empty, and the path is read once the caller
    /// and its grant are checked.
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) content_type: Option<String>,
    pub(crate) body: Vec<u8>,
    /// The surface's recorder, where it observes: what the metrics read, and what counts a reset.
    pub(crate) recorder: Option<Recorder>,
    /// The body `GET /api/served` answers, the surface's ([`Surface::with_console`]); `None` for
    /// a request no console route serves.
    pub(crate) served: Option<Arc<str>>,
    /// The caller the request proves in the header a surface made
    /// [`Surface::proving_callers`] reads ([`Caller::proven`]); `None` where it proves none, and
    /// for every request a [`Request`] value carries, which holds no header.
    pub(crate) proven: Option<Caller>,
}

impl Pending {
    /// What `GET /api/served` answers: `200` with the body the surface computed; `404`, as for a
    /// path no route has, where it has none.
    fn served_answer(&self) -> Response {
        match &self.served {
            Some(body) => Response {
                status: 200,
                allow: None,
                panel: false,
                metrics: false,
                body: body.to_string(),
            },
            None => Answer::refusal(404, format!("no route serves {}", self.path)).into(),
        }
    }

    /// The input the path carries for `route`: what the router read, or on a route whose
    /// requests prove their caller the path itself, still unread.
    fn carried<'a>(&'a self, route: &Route) -> Carried<'a> {
        if route.proves() {
            Carried::Unread(&self.path)
        } else {
            Carried::Read(&self.carried)
        }
    }

    /// Whether the request is a read `reader` answers beside the host's thread: a view whose
    /// owning store it reads ([`Reader::reads`]), `GET /api/model`, `GET /api/served`,
    /// `GET /api/events` where every store gives a reading handle ([`Reader::reads_events`]), and
    /// on an observing surface the metrics and the topology. A command, `POST /api/reset` and a view
    /// read on the host's thread only are answered there, one at a time in arrival order.
    #[cfg_attr(not(feature = "listener"), allow(dead_code))]
    pub(crate) fn reads(&self, reader: &Reader) -> bool {
        match &self.asked {
            Asked::Route(route) => match &route.operation {
                Operation::View(view) => reader.reads(view),
                Operation::Command(_) => false,
            },
            Asked::Console(Console::Model | Console::Served) => true,
            Asked::Console(Console::Events) => reader.reads_events(),
            Asked::Console(Console::Reset) => false,
            Asked::Hosted(_) => self.recorder.is_some(),
        }
    }

    /// Answers the request through `reader`, beside the host's thread, as [`Pending::serve`]
    /// answers it on that thread, the caller proven and the grants checked alike; the request
    /// back where it is no read [`Pending::reads`] names. A read asks no external cause and runs
    /// no binding, so none is noted and nothing is dropped.
    #[cfg_attr(not(feature = "listener"), allow(dead_code))]
    pub(crate) fn read(self, reader: &Reader) -> Result<(Response, Answered), Box<Self>> {
        if !self.reads(reader) {
            return Err(Box::new(self));
        }
        let read = match (&self.asked, &self.recorder) {
            (Asked::Route(route), _) => crate::answer::reading(
                reader,
                route,
                self.proven.as_ref(),
                self.carried(route),
                &self.query,
            )
            .map(|(answer, answered)| (answer.into(), answered)),
            (Asked::Console(Console::Model), _) => Some((
                Response {
                    status: 200,
                    allow: None,
                    panel: false,
                    metrics: false,
                    headers: Vec::new(),
                    body: crate::console::model(reader.ir()),
                },
                Answered::ingress(),
            )),
            (Asked::Console(Console::Events), _) => Some((
                crate::console::events_read_queried(reader, &self.query).into(),
                Answered::ingress(),
            )),
            (Asked::Console(Console::Served), _) => {
                Some((self.served_answer(), Answered::ingress()))
            }
            (Asked::Hosted(reserved), Some(recorder)) => Some((
                crate::observe::read_hosted(*reserved, recorder, reader),
                Answered::ingress(),
            )),
            (Asked::Console(Console::Reset) | Asked::Hosted(_), _) => None,
        };
        read.ok_or_else(|| Box::new(self))
    }

    /// Answers the request on `host`, a reset's world read by `worlds` where given, and says who
    /// answered it. Where the surface observes, every external cause asked from the request's
    /// start to its end, as the run's decider noted them ([`Recorder::answered`]), is answered
    /// with it in the order asked: by the routed command and by every command an event binding
    /// invoked on the way, each with the command that asked. One asked outside a request is
    /// counted and listed in no record.
    ///
    /// Every event binding whose input was unmet on the way is answered with it too
    /// ([`UnmetInput::of`]), in the order the host reported them.
    pub(crate) fn serve(
        self,
        host: &mut Host,
        worlds: Option<Worlds<'_>>,
    ) -> (Response, Vec<Failed>, Answered) {
        if let Some(recorder) = &self.recorder {
            recorder.serving();
        }
        let (response, failed, mut answered) = self.answer(host, worlds);
        if let Some(recorder) = &self.recorder {
            answered.externals = recorder.served();
        }
        answered.unmet = failed.iter().filter_map(UnmetInput::of).collect();
        (response, failed, answered)
    }

    fn answer(
        &self,
        host: &mut Host,
        worlds: Option<Worlds<'_>>,
    ) -> (Response, Vec<Failed>, Answered) {
        match &self.asked {
            Asked::Route(route) => {
                let (answer, failed, answered) = crate::answer::answering(
                    host,
                    route,
                    self.proven.as_ref(),
                    self.carried(route),
                    &self.query,
                    self.content_type.as_deref(),
                    &self.body,
                );
                (answer.into(), failed, answered)
            }
            Asked::Console(Console::Model) => (
                Response {
                    status: 200,
                    allow: None,
                    panel: false,
                    metrics: false,
                    headers: Vec::new(),
                    body: crate::console::model(host.ir()),
                },
                Vec::new(),
                Answered::ingress(),
            ),
            Asked::Console(Console::Events) => (
                crate::console::events_queried(host, &self.query).into(),
                Vec::new(),
                Answered::ingress(),
            ),
            Asked::Console(Console::Served) => {
                (self.served_answer(), Vec::new(), Answered::ingress())
            }
            Asked::Console(Console::Reset) => {
                let answer = crate::console::reset(host, &self.body, worlds);
                if answer.status == 200
                    && let Some(recorder) = &self.recorder
                {
                    recorder.count_reset();
                }
                (answer.into(), Vec::new(), Answered::ingress())
            }
            Asked::Hosted(reserved) => (
                match &self.recorder {
                    Some(recorder) => crate::observe::hosted(*reserved, recorder, host),
                    None => {
                        Answer::refusal(404, "no route serves this path under /__spindle/").into()
                    }
                },
                Vec::new(),
                Answered::ingress(),
            ),
        }
    }
}
