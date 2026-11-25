//! The `ess-http/1` shapes ESS 0.99.0 added, answered by [`spindle_ingress::route`] as ESS's
//! served application answers them where this ingress serves them, and named not served where it
//! does not.
//!
//! Served: a command body under one member (`{<member>: response}`, `{<member>: response.<field>}`,
//! beyond10x/ess#755); `unknown: ignore` beside a `json` body, which drops a member no input field
//! names and holds a member the path carries to the path's value (beyond10x/ess#752); and, under
//! `unknown: ignore` on a `query` route, a key the path carries held to the path's value, where
//! 0.98.0 dropped it whatever it held (beyond10x/ess#752). A view answered under one member
//! (beyond10x/ess#755) is served too: `binding_shapes`.
//!
//! Not served, each route named with why: a `multipart/form-data` request and `request.query`
//! (beyond10x/ess#754), a request body limit other than the 1 MiB this ingress reads and a view
//! with a CSV representation (beyond10x/ess#753). A
//! binding-wide limit other than 1 MiB refuses the whole binding, as a binding-wide refusal does.
//!
//! The fixture `tests/fixtures/kiosk` declares one component, `kiosk`, reached over the network:
//! `IssueTicket` creates a ticket and returns a minted `code` and `seat`, `PunchTicket` sets an
//! issued ticket's punches, `StampTicket` its holder, and `AllTickets` lists them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Request, Response, Surface};

const COMPONENT: &str = "kiosk";
const JSON: &str = "application/json";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("kiosk"))
        .expect("the kiosk fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"ess-http-shapes\"\n",
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
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// The binding `routes` (YAML, indented under `routes:`) and `top` (top-level keys) declare,
/// compiled by ESS against `ir`.
fn binding(ir: &EssIr, top: &str, routes: &str) -> ess_gen::http::Binding {
    let text = format!(
        "type: ess-http/1
specification:
  system: kiosk
  version: v1
  source_digest: sha256:{}
{top}routes:
{routes}",
        ir.source_digest()
    );
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}\n{text}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

fn surface(ir: &EssIr, binding: &ess_gen::http::Binding) -> Surface {
    Surface::new(ir, &[COMPONENT.to_owned()], Some(binding))
        .unwrap_or_else(|refused| panic!("the kiosk surface is served: {refused}"))
}

/// Why the surface does not serve `operation`; `None` where it serves it.
fn not_served(surface: &Surface, operation: &str) -> Option<String> {
    surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("the surface routes {operation}"))
        .not_served()
}

fn send(
    surface: &Surface,
    host: &mut Host,
    method: &str,
    target: &str,
    body: Option<Value>,
) -> Response {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    spindle_ingress::route(
        surface,
        host,
        Request {
            method: method.to_owned(),
            path: path.to_owned(),
            query: query.to_owned(),
            content_type: body.as_ref().map(|_| JSON.to_owned()),
            body: body
                .map(|body| body.to_string().into_bytes())
                .unwrap_or_default(),
        },
    )
    .response
}

fn parsed(response: &Response) -> Value {
    serde_json::from_str(&response.body)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", response.body))
}

/// The routes every served case uses: the issue answered as `issue` declares it, the punch read
/// from a `json` body and the stamp from the query string, both ignoring unknown members.
fn served_routes(issue: &str) -> String {
    format!(
        "  - command: kiosk.ticket.IssueTicket
    method: post
    path: /tickets
    request: {{unknown: ignore}}
    outcomes:
      issued: {{status: 201, body: {issue}}}
  - command: kiosk.ticket.PunchTicket
    method: put
    path: /tickets/{{id}}/punches
    parameters: {{id: input.ticket_id}}
    request: {{encoding: json, unknown: ignore}}
    outcomes:
      punched: {{status: 200}}
  - command: kiosk.ticket.StampTicket
    method: delete
    path: /tickets/{{id}}/stamp
    parameters: {{id: input.ticket_id}}
    request: {{encoding: query, unknown: ignore}}
    outcomes:
      stamped: {{status: 200}}
  - view: kiosk.ticket.AllTickets
    method: get
    path: /tickets
"
    )
}

fn rows(surface: &Surface, host: &mut Host) -> Value {
    let response = send(surface, host, "GET", "/tickets", None);
    assert_eq!(response.status, 200, "{}", response.body);
    parsed(&response)["rows"].clone()
}

#[test]
fn a_wrapped_body_holds_the_response_or_one_field_under_its_member() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    for (issue, member, field) in [
        ("{ticket: response}", "ticket", None),
        ("{code: response.code}", "code", Some("code")),
    ] {
        let bound = binding(&ir, "", &served_routes(issue));
        let surface = surface(&ir, &bound);
        assert_eq!(not_served(&surface, "kiosk.ticket.IssueTicket"), None);
        let mut host = Host::new(
            ir.clone(),
            &in_memory(),
            Arc::new(ManualClock::new()),
            &state.path().join(member),
        )
        .expect("the host admits the kiosk");
        let response = send(
            &surface,
            &mut host,
            "POST",
            "/tickets",
            Some(json!({"ticket_id": "t1", "holder": "ada"})),
        );
        assert_eq!(response.status, 201, "{issue}: {}", response.body);
        let body = parsed(&response);
        let object = body
            .as_object()
            .unwrap_or_else(|| panic!("{issue}: not an object: {body}"));
        assert_eq!(
            object.keys().collect::<Vec<_>>(),
            [member],
            "{issue}: {body}"
        );
        match field {
            None => {
                let response = &object[member];
                assert!(response["code"].is_string(), "{issue}: {body}");
                assert!(response["seat"].is_string(), "{issue}: {body}");
            }
            Some(_) => assert!(object[member].is_string(), "{issue}: {body}"),
        }
    }
}

#[test]
fn a_json_body_ignoring_unknown_members_holds_a_path_echo_to_the_paths_value() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let bound = binding(&ir, "", &served_routes("response"));
    let surface = surface(&ir, &bound);
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .expect("the host admits the kiosk");
    let issued = send(
        &surface,
        &mut host,
        "POST",
        "/tickets",
        Some(json!({"ticket_id": "t1", "holder": "ada", "colour": "red", "seat": {"row": 1}})),
    );
    assert_eq!(
        issued.status, 201,
        "unknown members are dropped: {}",
        issued.body
    );

    let echoed = send(
        &surface,
        &mut host,
        "PUT",
        "/tickets/t1/punches",
        Some(json!({"ticket_id": "t2", "punches": 3})),
    );
    assert_eq!(echoed.status, 400, "{}", echoed.body);
    assert!(
        echoed
            .body
            .contains("body.ticket_id: the path carries this field with another value"),
        "{}",
        echoed.body
    );
    assert_eq!(rows(&surface, &mut host)[0]["punches"], 0);

    let punched = send(
        &surface,
        &mut host,
        "PUT",
        "/tickets/t1/punches",
        Some(json!({"ticket_id": "t1", "punches": 3, "note": "front"})),
    );
    assert_eq!(punched.status, 200, "{}", punched.body);
    assert_eq!(
        rows(&surface, &mut host),
        json!([{"ticket_id": "t1", "holder": "ada", "punches": 3}])
    );
}

#[test]
fn a_query_key_the_path_carries_is_dropped_only_when_it_holds_the_paths_value() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let bound = binding(&ir, "", &served_routes("response"));
    let surface = surface(&ir, &bound);
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .expect("the host admits the kiosk");
    let issued = send(
        &surface,
        &mut host,
        "POST",
        "/tickets",
        Some(json!({"ticket_id": "t1", "holder": "ada"})),
    );
    assert_eq!(issued.status, 201, "{}", issued.body);

    let echoed = send(
        &surface,
        &mut host,
        "DELETE",
        "/tickets/t1/stamp?ticket_id=t9&holder=bob",
        None,
    );
    assert_eq!(echoed.status, 400, "{}", echoed.body);
    assert!(
        echoed
            .body
            .contains("query.ticket_id: the path carries this field with another value"),
        "{}",
        echoed.body
    );
    assert_eq!(rows(&surface, &mut host)[0]["holder"], "ada");

    let stamped = send(
        &surface,
        &mut host,
        "DELETE",
        "/tickets/t1/stamp?ticket_id=t1&holder=bob&colour=red&%ZZ=1",
        None,
    );
    assert_eq!(stamped.status, 200, "{}", stamped.body);
    assert_eq!(rows(&surface, &mut host)[0]["holder"], "bob");
}

#[test]
fn each_shape_this_ingress_does_not_read_is_named_not_served() {
    let ir = ir();
    for (routes, operation, why) in [
        (
            "  - command: kiosk.ticket.StampTicket
    method: post
    path: /tickets/{id}/stamp
    parameters: {id: input.ticket_id}
    request: {encoding: multipart}
    outcomes:
      stamped: {status: 200}
",
            "kiosk.ticket.StampTicket",
            "a `multipart/form-data` request",
        ),
        (
            "  - command: kiosk.ticket.IssueTicket
    method: post
    path: /tickets
    request: {query: [input.holder]}
    outcomes:
      issued: {status: 201}
",
            "kiosk.ticket.IssueTicket",
            "input fields read from the query string beside the body",
        ),
        (
            "  - command: kiosk.ticket.IssueTicket
    method: post
    path: /tickets
    limits: {request_bytes: 1024}
    outcomes:
      issued: {status: 201}
",
            "kiosk.ticket.IssueTicket",
            "a request body limit of 1024 bytes, where this ingress reads 1048576",
        ),
        (
            "  - view: kiosk.ticket.AllTickets
    method: get
    path: /tickets
    representations:
      - query: {format: csv}
        encoding: {csv: {separator: \";\"}}
",
            "kiosk.ticket.AllTickets",
            "a view whose query selects another representation of its rows, such as CSV",
        ),
    ] {
        let bound = binding(&ir, "", routes);
        let surface = surface(&ir, &bound);
        let line = not_served(&surface, operation)
            .unwrap_or_else(|| panic!("{operation} is served under:\n{routes}"));
        assert!(line.ends_with(why), "{line}\n{routes}");
    }
}

#[test]
fn a_binding_wide_body_limit_is_served_only_at_this_ingresss_own() {
    let ir = ir();
    let routes = served_routes("response");
    let own = binding(&ir, "limits: {request_bytes: 1048576}\n", &routes);
    let surface = surface(&ir, &own);
    assert_eq!(not_served(&surface, "kiosk.ticket.IssueTicket"), None);

    let other = binding(&ir, "limits: {request_bytes: 2048}\n", &routes);
    let Err(refused) = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&other)) else {
        panic!("a binding-wide limit of 2048 bytes is refused");
    };
    assert!(
        refused
            .to_string()
            .contains("a request body limit of 2048 bytes, where this ingress reads 1048576"),
        "{refused}"
    );
}
