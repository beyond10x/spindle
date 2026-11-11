//! The grant check of a served command or read-granted view, against ESS's served surface
//! (`ess-synth` `rust/http/bound.rs`, `answer_call`: the grant is checked, then the request is
//! answered).
//!
//! `story:command-grant-before-request`: a command whose caller is not granted is refused `403`
//! before its body, query string or path is read; the transport limits stay before the check.
//! `story:unauthenticated-grant-refusals`: without a fixed caller, a read-granted view and a
//! command some actor's `may:` names are refused `403` `{"refused": "not granted", "actor": null}`,
//! and nothing is read or written; a view and a command no grant names are served as before.
//!
//! The fixture `tests/fixtures/grants` declares one component, `ledger`: `vault.ledger.Clerk` may
//! file (a `form` body), stamp (the query string) and note (a JSON body) an entry, each carrying
//! the entry's number in the path (`tests/fixtures/grants-http.yaml`), and read the entries a title
//! selects; `vault.ledger.Visitor` may only visit. No grant names `SweepEntry` or `AllEntries`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{BODY_LIMIT, Caller, Listener, Request, Surface};

const COMPONENT: &str = "ledger";
const CLERK: &str = "vault.ledger.Clerk";
const VISITOR: &str = "vault.ledger.Visitor";
const TITLED: &str = "vault.ledger.EntriesTitled";
const ALL: &str = "vault.ledger.AllEntries";
const SWEEP: &str = "vault.ledger.SweepEntry";
const FORM: &str = "application/x-www-form-urlencoded";

const STEPS: usize = 50;
const WAIT: Duration = Duration::from_millis(200);
const HEADER: &str = "x-vault-caller";
const SECRET: &str = "proxy-secret";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("grants"))
        .unwrap_or_else(|error| panic!("the grants fixture compiles: {error:?}"))
        .ir
}

fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = std::fs::read_to_string(fixtures().join("grants-http.yaml")).expect("the binding");
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"grants\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn caller(actor: &str) -> Caller {
    Caller {
        actor: actor.to_owned(),
        attributes: Map::new(),
    }
}

/// The ledger's surface, every operation sent or read as `fixed` where given, else as no caller.
fn surface(ir: &EssIr, fixed: Option<&str>) -> Surface {
    let callers: Vec<Caller> = fixed.into_iter().map(caller).collect();
    Surface::fixing_callers(ir, &[COMPONENT.to_owned()], Some(&binding(ir)), &callers)
        .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
}

struct Ledger {
    surface: Surface,
    host: Host,
    _state: tempfile::TempDir,
}

impl Ledger {
    fn open(fixed: Option<&str>) -> Self {
        let ir = ir();
        let surface = surface(&ir, fixed);
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
        Self {
            surface,
            host,
            _state: state,
        }
    }

    /// The method and path the surface serves `operation` at.
    fn at(&self, operation: &str) -> (String, String) {
        let route = self
            .surface
            .routes()
            .iter()
            .find(|route| route.operation.name() == operation)
            .unwrap_or_else(|| panic!("the surface serves {operation}"));
        (route.method.as_str().to_uppercase(), route.path.clone())
    }

    fn send(&mut self, sent: &Sent<'_>) -> (u16, Value) {
        let routed = spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: sent.method.to_owned(),
                path: sent.path.to_owned(),
                query: sent.query.to_owned(),
                content_type: sent.content_type.map(str::to_owned),
                body: sent.body.to_vec(),
            },
        );
        let body = if routed.response.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&routed.response.body)
                .unwrap_or_else(|_| panic!("not JSON: {}", routed.response.body))
        };
        (routed.response.status, body)
    }

    /// Every entry the host holds, read by the open view.
    fn entries(&mut self) -> Value {
        let (method, path) = self.at(ALL);
        let (status, body) = self.send(&Sent {
            method: &method,
            path: &path,
            query: "",
            content_type: None,
            body: b"",
        });
        assert_eq!(status, 200, "the open view is served: {body}");
        body["rows"].clone()
    }
}

/// One request, as a value.
#[derive(Debug, Clone, Copy)]
struct Sent<'a> {
    method: &'a str,
    path: &'a str,
    query: &'a str,
    content_type: Option<&'a str>,
    body: &'a [u8],
}

/// Requests to the clerk's commands that a granted caller is refused `400`: a malformed JSON
/// body, a malformed `form` body, a malformed query string, and a path whose number is no
/// integer, for each encoding.
const MALFORMED: [Sent<'static>; 12] = [
    Sent {
        method: "POST",
        path: "/notes/1",
        query: "",
        content_type: Some("application/json"),
        body: b"{not json",
    },
    Sent {
        method: "POST",
        path: "/notes/1",
        query: "",
        content_type: Some("application/json"),
        body: b"[\"title\"]",
    },
    Sent {
        method: "POST",
        path: "/notes/1",
        query: "",
        content_type: Some("application/json"),
        body: b"{\"title\": \"a\", \"stray\": 1}",
    },
    Sent {
        method: "POST",
        path: "/notes/one",
        query: "",
        content_type: Some("application/json"),
        body: b"{\"title\": \"a\"}",
    },
    Sent {
        method: "POST",
        path: "/entries/1",
        query: "",
        content_type: Some("application/json"),
        body: b"title=a",
    },
    Sent {
        method: "POST",
        path: "/entries/1",
        query: "",
        content_type: Some(FORM),
        body: b"title=%ZZ",
    },
    Sent {
        method: "POST",
        path: "/entries/1",
        query: "",
        content_type: Some(FORM),
        body: b"title=a&stray=1",
    },
    Sent {
        method: "POST",
        path: "/entries/one",
        query: "",
        content_type: Some(FORM),
        body: b"title=a",
    },
    Sent {
        method: "GET",
        path: "/entries/1/stamp",
        query: "title=%ZZ",
        content_type: None,
        body: b"",
    },
    Sent {
        method: "GET",
        path: "/entries/1/stamp",
        query: "title=a&stray=1",
        content_type: None,
        body: b"",
    },
    Sent {
        method: "GET",
        path: "/entries/1/stamp",
        query: "title=a&number=2",
        content_type: None,
        body: b"",
    },
    Sent {
        method: "GET",
        path: "/entries/one/stamp",
        query: "title=a",
        content_type: None,
        body: b"",
    },
];

/// Well-formed requests to each of the clerk's commands.
const WELL_FORMED: [Sent<'static>; 3] = [
    Sent {
        method: "POST",
        path: "/notes/1",
        query: "",
        content_type: Some("application/json"),
        body: b"{\"title\": \"a\"}",
    },
    Sent {
        method: "POST",
        path: "/entries/2",
        query: "",
        content_type: Some(FORM),
        body: b"title=a",
    },
    Sent {
        method: "GET",
        path: "/entries/3/stamp",
        query: "title=a",
        content_type: None,
        body: b"",
    },
];

#[test]
fn an_ungranted_fixed_caller_sending_a_malformed_command_request_is_refused_403() {
    let mut ledger = Ledger::open(Some(VISITOR));
    for sent in MALFORMED.iter().chain(&WELL_FORMED) {
        let (status, body) = ledger.send(sent);
        assert_eq!(
            (status, &body),
            (403, &json!({"refused": "not granted", "actor": VISITOR})),
            "{sent:?}: the grant is checked before the request is read"
        );
    }
    assert_eq!(ledger.entries(), json!([]), "nothing is written");
}

#[test]
fn a_granted_fixed_caller_sending_the_same_malformed_request_gets_400() {
    let mut ledger = Ledger::open(Some(CLERK));
    for sent in &MALFORMED {
        let (status, body) = ledger.send(sent);
        assert_eq!(status, 400, "{sent:?}: {body}");
    }
    assert_eq!(ledger.entries(), json!([]), "nothing is written");
    for sent in &WELL_FORMED {
        let (status, body) = ledger.send(sent);
        assert!((200..300).contains(&status), "{sent:?}: {status} {body}");
    }
}

#[test]
fn a_granted_command_without_a_caller_is_refused_403() {
    let mut ledger = Ledger::open(None);
    for sent in MALFORMED.iter().chain(&WELL_FORMED) {
        let (status, body) = ledger.send(sent);
        assert_eq!(
            (status, &body),
            (403, &json!({"refused": "not granted", "actor": null})),
            "{sent:?}: a command some actor's grant names proves no actor here"
        );
    }
    assert_eq!(ledger.entries(), json!([]), "nothing is written");
}

#[test]
fn a_read_granted_view_without_a_caller_is_refused_403() {
    let mut ledger = Ledger::open(None);
    let (method, path) = ledger.at(SWEEP);
    let (status, body) = ledger.send(&Sent {
        method: &method,
        path: &path,
        query: "",
        content_type: Some("application/json"),
        body: b"{\"number\": 7, \"title\": \"a\"}",
    });
    assert!((200..300).contains(&status), "the sweep runs: {body}");
    let (method, path) = ledger.at(TITLED);
    for query in ["title=a", "title=%ZZ", "", "stray=1"] {
        let (status, body) = ledger.send(&Sent {
            method: &method,
            path: &path,
            query,
            content_type: None,
            body: b"",
        });
        assert_eq!(
            (status, &body),
            (403, &json!({"refused": "not granted", "actor": null})),
            "?{query}: nothing is read"
        );
    }
}

#[test]
fn an_ungranted_view_and_command_without_a_caller_are_served_as_before() {
    let mut ledger = Ledger::open(None);
    let (method, path) = ledger.at(SWEEP);
    let (status, body) = ledger.send(&Sent {
        method: &method,
        path: &path,
        query: "",
        content_type: Some("application/json"),
        body: b"{\"number\": 7, \"title\": \"a\"}",
    });
    assert!((200..300).contains(&status), "the sweep runs: {body}");
    assert_eq!(ledger.entries(), json!([{"number": 7, "title": "a"}]));
}

/// The surface each request proves its own caller on, as `spindle run --trusted-caller-header`
/// serves it.
fn proving(ir: &EssIr) -> Surface {
    Surface::proving_callers(
        ir,
        &[COMPONENT.to_owned()],
        Some(&binding(ir)),
        HEADER,
        SECRET,
    )
    .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
}

#[test]
fn an_ungranted_proven_caller_sending_a_malformed_command_request_is_refused_403() {
    let ir = ir();
    let listener = Listener::bind(0, proving(&ir)).expect("the listener binds");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let proof = |actor: &str| {
        format!(
            "{HEADER}: {{\"actor\": \"{actor}\"}}\r\n{}: {SECRET}\r\n",
            Surface::SECRET_HEADER
        )
    };
    for sent in &MALFORMED {
        let (status, body) = served(&listener, &mut host, raw(sent, &proof(VISITOR)));
        assert_eq!(
            (status, &body),
            (403, &json!({"refused": "not granted", "actor": VISITOR})),
            "{sent:?}: the proven caller's grant is checked before its request is read"
        );
        let (status, body) = served(&listener, &mut host, raw(sent, &proof(CLERK)));
        assert_eq!(status, 400, "{sent:?}: {body}");
        let (status, body) = served(&listener, &mut host, raw(sent, ""));
        assert_eq!(
            (status, &body),
            (403, &json!({"refused": "not granted", "actor": null})),
            "{sent:?}: a request that proves no caller"
        );
    }
}

#[test]
fn the_transport_limits_stay_before_the_grant_check() {
    let mut ledger = Ledger::open(Some(VISITOR));
    let oversized = vec![b' '; BODY_LIMIT + 1];
    let (status, body) = ledger.send(&Sent {
        body: &oversized,
        ..WELL_FORMED[0]
    });
    assert_eq!(status, 413, "a body over the limit: {body}");

    let ir = ir();
    let listener = Listener::bind(0, surface(&ir, Some(VISITOR))).expect("the listener binds");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let address = listener.local_addr();
    let chunked = format!(
        "POST /notes/1 HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nTransfer-Encoding: \
         chunked\r\nContent-Type: application/json\r\n\r\n2\r\n{{}}\r\n0\r\n\r\n"
    );
    let (status, body) = served(&listener, &mut host, chunked);
    assert_eq!(status, 411, "a chunked body: {body}");
    let announced = format!(
        "POST /notes/1 HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: \
         {}\r\nContent-Type: application/json\r\n\r\n",
        BODY_LIMIT + 1
    );
    let (status, body) = served(&listener, &mut host, announced);
    assert_eq!(status, 413, "a body announced over the limit: {body}");
    // Ten bytes announced and none sent: the listener refuses the request once `BODY_READ`
    // passes, before the host is handed anything.
    let late = format!(
        "POST /notes/1 HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: \
         10\r\nContent-Type: application/json\r\n\r\n"
    );
    let (status, body) = exchange(address, &late);
    assert_eq!(status, 408, "a body that does not arrive: {body}");
}

/// `sent` as HTTP/1.1 text, with the header lines `headers` (each ending in CRLF).
fn raw(sent: &Sent<'_>, headers: &str) -> String {
    let target = if sent.query.is_empty() {
        sent.path.to_owned()
    } else {
        format!("{}?{}", sent.path, sent.query)
    };
    let content_type = sent
        .content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "{} {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{headers}{content_type}\
         Content-Length: {}\r\n\r\n{}",
        sent.method,
        sent.body.len(),
        String::from_utf8_lossy(sent.body)
    )
}

fn served(listener: &Listener, host: &mut Host, text: String) -> (u16, Value) {
    let address = listener.local_addr();
    let client = thread::spawn(move || exchange(address, &text));
    for _ in 0..STEPS {
        if client.is_finished() {
            break;
        }
        if let Some(exchange) = listener.next(WAIT) {
            exchange.serve(host);
        }
    }
    assert!(client.is_finished(), "the request was not answered in time");
    client.join().expect("the client thread")
}

fn exchange(address: SocketAddr, text: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("a read timeout");
    stream
        .write_all(text.as_bytes())
        .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    let body = serde_json::from_str(body).unwrap_or_else(|_| panic!("not JSON: {body}"));
    (status, body)
}
