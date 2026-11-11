//! A command outcome whose `ess-http/1` binding declares response headers answers over HTTP with
//! those headers beside its declared status and body, as ESS's served application writes them
//! (`story:outcome-response-headers`).
//!
//! The fixture `tests/fixtures/door` declares one component, `door`, reached over the network.
//! `PassDoor` refuses a pass that is not accepted with `PassRefused` (wire name `invalid_token`),
//! whose `description` (wire name `error_description`) the outcome sets from the input's `why`,
//! and lets every other pass through. The binding below answers the refusal `401` with the error
//! object, a `WWW-Authenticate` `Bearer` challenge and an `X-Realm` header read from the input's
//! optional `realm`, and the pass `200` with the envelope and a literal `Cache-Control`. Every
//! answer is checked against the document `ess_gen::openapi::bound_json` publishes for the
//! component with that binding.
//!
//! A value read from the request that a header cannot carry (a control character other than a
//! horizontal tab, or a space or tab at either end of a plain header) is never written, cut or
//! re-encoded: the request is answered `501` naming the header, as ESS's served application
//! refuses it (`ess-synth` `http::push_header`, `http::challenge`).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "door";
const PASS: &str = "keep.door.PassDoor";
const PATH: &str = "/doors/passes";

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("door"))
        .expect("the door fixture compiles")
        .ir
}

/// The binding: the refusal `401` with the error object, a challenge and a header from the input;
/// the pass `200` with the envelope and a literal header.
fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = format!(
        "type: ess-http/1
specification:
  system: keep
  version: v1
  source_digest: sha256:{}
routes:
  - command: {PASS}
    method: post
    path: {PATH}
    outcomes:
      refused:
        status: 401
        body: {{error: error}}
        headers:
          WWW-Authenticate:
            challenge:
              scheme: Bearer
              params:
                realm: {{literal: keep}}
                error: {{source: error.wire}}
                error_description: {{source: error.description}}
          X-Realm: {{source: input.realm}}
      passed:
        status: 200
        headers:
          Cache-Control: {{literal: no-store}}
",
        ir.source_digest()
    );
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

/// An environment that binds every store to `memory`.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"outcome-headers\"\n",
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

/// One answer as read off the wire: the status, every header line as `(name, value)` with the
/// name lower-cased, and the body.
#[derive(Debug)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    /// Every value of the header `name`, in the order written.
    fn all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// The one value of the header `name`; `None` where it is absent, and a failure where it is
    /// written more than once.
    fn header(&self, name: &str) -> Option<&str> {
        let all = self.all(name);
        assert!(
            all.len() <= 1,
            "{name} written {} times: {self:?}",
            all.len()
        );
        all.first().copied()
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {self:?}"))
    }
}

/// The `OpenAPI` document ESS publishes for the component with the binding.
struct Contract(Value);

impl Contract {
    fn of(ir: &EssIr, binding: &ess_gen::http::Binding) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::bound_json(ir, component, Some(binding));
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The response the contract declares for the route at `status`.
    fn response(&self, status: u16) -> &Value {
        let response = &self.0["paths"][PATH]["post"]["responses"][status.to_string()];
        assert!(!response.is_null(), "the contract declares no {status}");
        response
    }

    /// Fails unless `reply`'s body validates against the schema the contract declares for its
    /// status, and every header the contract requires under that status is written, a `const`
    /// one with exactly its value.
    fn validate(&self, reply: &Reply) {
        let response = self.response(reply.status);
        let schema = &response["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "no JSON {} answer: {response}",
            reply.status
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let body = reply.json();
        let errors = validator
            .iter_errors(&body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{reply:?}: {errors:?}");
        for (name, declared) in response["headers"].as_object().into_iter().flatten() {
            let written = reply.header(&name.to_ascii_lowercase());
            if declared["required"] == true {
                let written = written.unwrap_or_else(|| panic!("{name} is not written: {reply:?}"));
                if let Some(constant) = declared["schema"]["const"].as_str() {
                    assert_eq!(written, constant, "{name}: {reply:?}");
                }
            }
        }
    }
}

/// The door, served by a listener on the binding.
struct Door {
    host: Host,
    listener: Listener,
    contract: Contract,
    _state: tempfile::TempDir,
}

impl Door {
    fn open() -> Self {
        let ir = ir();
        let binding = binding(&ir);
        let contract = Contract::of(&ir, &binding);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&binding))
            .unwrap_or_else(|refused| panic!("the door surface is served: {refused}"));
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the door: {error}"));
        Self {
            host,
            listener,
            contract,
            _state: state,
        }
    }

    /// Sends `input` to the route and serves it on the host.
    fn send(&mut self, input: &Value) -> Reply {
        let address = self.listener.local_addr();
        let body = input.to_string();
        let client = thread::spawn(move || request(address, &body));
        for _ in 0..STEPS {
            if client.is_finished() {
                break;
            }
            if let Some(exchange) = self.listener.next(WAIT) {
                exchange.serve(&mut self.host);
            }
        }
        assert!(client.is_finished(), "the request was not answered in time");
        client.join().expect("the client thread")
    }
}

/// One HTTP/1.1 `POST` of `body` as JSON, `Connection: close`, read back whole.
fn request(address: SocketAddr, body: &str) -> Reply {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "POST {PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("the response is read");
    let response = String::from_utf8(response).expect("the response is UTF-8");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a head: {response:?}"));
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head:?}"));
    let headers = lines
        .map(|line| {
            let (name, value) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("a header line without a `:`: {line:?} in {head:?}"));
            (name.to_ascii_lowercase(), value.trim().to_owned())
        })
        .collect();
    Reply {
        status,
        headers,
        body: body.to_owned(),
    }
}

fn refused(why: &str, realm: Option<&str>) -> Value {
    let mut input = json!({"door_id": "d1", "accepted": false, "why": why});
    if let Some(realm) = realm {
        input["realm"] = Value::String(realm.to_owned());
    }
    input
}

/// The acceptance: the refusal bound to `WWW-Authenticate` answers the declared status, the
/// declared body and the challenge with the value the binding declares, as ESS's projection
/// describes it; the header read from the input carries the input's value.
#[test]
fn a_refusal_bound_to_a_challenge_answers_it_with_the_declared_status_and_body() {
    let mut door = Door::open();
    let reply = door.send(&refused("the pass has expired", Some("north")));
    assert_eq!(reply.status, 401, "the declared status: {reply:?}");
    assert_eq!(
        reply.json(),
        json!({"error": "invalid_token", "error_description": "the pass has expired"}),
        "the declared body: {reply:?}"
    );
    assert_eq!(
        reply.header("www-authenticate"),
        Some(
            r#"Bearer error="invalid_token", error_description="the pass has expired", realm="keep""#
        ),
        "each param a quoted string, in name order: {reply:?}"
    );
    assert_eq!(reply.header("x-realm"), Some("north"), "{reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    door.contract.validate(&reply);
    let declared = &door.contract.response(401)["headers"];
    assert!(
        declared.get("WWW-Authenticate").is_some() && declared.get("X-Realm").is_some(),
        "the projection declares both headers under 401: {declared}"
    );
}

#[test]
fn a_header_whose_optional_source_is_absent_is_not_written() {
    let mut door = Door::open();
    let reply = door.send(&refused("no pass", None));
    assert_eq!(reply.status, 401, "{reply:?}");
    assert_eq!(reply.header("x-realm"), None, "{reply:?}");
    assert!(reply.header("www-authenticate").is_some(), "{reply:?}");
    door.contract.validate(&reply);
}

#[test]
fn a_success_outcome_answers_its_literal_header_beside_the_envelope() {
    let mut door = Door::open();
    let reply = door.send(&json!({"door_id": "d1", "accepted": true, "why": "-"}));
    assert_eq!(reply.status, 200, "{reply:?}");
    assert_eq!(reply.json()["outcome"], "passed", "{reply:?}");
    assert_eq!(reply.header("cache-control"), Some("no-store"), "{reply:?}");
    door.contract.validate(&reply);
}

/// A quoted string's `"` and `\` are escaped, so a value from the request cannot leave it.
#[test]
fn a_challenge_param_from_the_request_is_escaped_inside_its_quoted_string() {
    let mut door = Door::open();
    let reply = door.send(&refused(r#"a "quoted" \ word", realm="x"#, None));
    assert_eq!(reply.status, 401, "{reply:?}");
    assert_eq!(
        reply.header("www-authenticate"),
        Some(
            r#"Bearer error="invalid_token", error_description="a \"quoted\" \\ word\", realm=\"x", realm="keep""#
        ),
        "{reply:?}"
    );
}

/// A value from the request that would end its header line, or that a header cannot carry, is
/// never written: the request is answered `501` naming the header, and no header the value spells
/// reaches the wire.
#[test]
fn a_header_value_from_the_request_that_a_header_cannot_carry_is_refused() {
    let mut door = Door::open();
    for (why, realm, named) in [
        ("fine", Some("north\r\nSet-Cookie: taken=1"), "X-Realm"),
        ("fine", Some("north\nSet-Cookie: taken=1"), "X-Realm"),
        ("fine", Some("north\rSet-Cookie: taken=1"), "X-Realm"),
        ("fine", Some("nul\u{0}"), "X-Realm"),
        ("fine", Some("del\u{7f}"), "X-Realm"),
        ("fine", Some("c1\u{85}"), "X-Realm"),
        ("fine", Some(" north"), "X-Realm"),
        ("fine", Some("north\t"), "X-Realm"),
        (
            "x\r\nSet-Cookie: taken=1",
            Some("north"),
            "error_description",
        ),
        ("x\nSet-Cookie: taken=1", None, "error_description"),
        ("bell\u{7}", None, "error_description"),
    ] {
        let reply = door.send(&refused(why, realm));
        assert_eq!(reply.status, 501, "{why:?} {realm:?}: {reply:?}");
        assert!(
            reply.all("set-cookie").is_empty(),
            "no header the value spells is written: {reply:?}"
        );
        assert_eq!(
            reply.header("www-authenticate"),
            None,
            "none of the declared headers is written: {reply:?}"
        );
        assert_eq!(reply.header("x-realm"), None, "{reply:?}");
        let body = reply.json();
        let refusal = body["refused"].as_str().unwrap_or_else(|| panic!("{body}"));
        assert!(refusal.contains(named), "names {named}: {refusal}");
        assert_eq!(body["committed"], false, "a refusal writes nothing: {body}");
    }
    let reply = door.send(&refused("ok", Some("in\tside")));
    assert_eq!(
        reply.header("x-realm"),
        Some("in\tside"),
        "a tab inside a value is field-value text: {reply:?}"
    );
}
