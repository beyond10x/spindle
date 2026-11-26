//! A command whose outcome emits an event its component does not publish is served
//! (`story:command-with-unpublished-event-served`): the route answers the outcome, the event is
//! kept with the write and listed by [`Host::published`] as ESS's interpreter lists it, and a
//! published event beside it is sent in the same command.
//!
//! The fixture `tests/fixtures/unpublished` declares two components reached by network. `desk`
//! owns `helpdesk.tickets` and publishes only `Touched`: `OpenTicket` creates a ticket under the
//! `ticket_id` of the `Opened` it emits, and `TouchTicket` emits `Touched`, then `Noted`. `log`
//! owns `helpdesk.log` and binds `Touched` to `RecordMark`, which keeps one `Mark` per touch,
//! listed by `Marks`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::{ConformanceTarget, ScenarioContext, ScenarioId, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Request, Response, Surface};

const DESK: &str = "desk";
const OPEN: &str = "helpdesk.tickets.OpenTicket";
const OPENED: &str = "helpdesk.tickets.Opened";
const TOUCHED: &str = "helpdesk.tickets.Touched";
const NOTED: &str = "helpdesk.tickets.Noted";
const MARKED: &str = "helpdesk.log.Marked";
const MARKS: &str = "helpdesk.log.Marks";

fn ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unpublished"))
        .unwrap_or_else(|error| panic!("ESS 0.99.1 compiles the unpublished fixture: {error}"))
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"unpublished-event\"\n",
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

/// The desk's binding, compiled by ESS against `ir`: `opened` answered 201, `touched` 200.
fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = format!(
        "type: ess-http/1
specification:
  system: helpdesk
  version: v1
  source_digest: sha256:{}
routes:
  - command: helpdesk.tickets.OpenTicket
    method: post
    path: /tickets
    outcomes:
      opened: {{status: 201}}
  - command: helpdesk.tickets.TouchTicket
    method: post
    path: /tickets/{{id}}/touches
    parameters: {{id: input.ticket_id}}
    outcomes:
      touched: {{status: 200}}
",
        ir.source_digest()
    );
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}\n{text}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

/// The desk served by the binding, and a host running both components.
struct Desk {
    surface: Surface,
    host: Host,
    _state: tempfile::TempDir,
}

impl Desk {
    fn new(ir: &EssIr) -> Self {
        let bound = binding(ir);
        let surface = Surface::new(ir, &[DESK.to_owned()], Some(&bound))
            .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"));
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(
            ir.clone(),
            &in_memory(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("the host admits the help desk: {error}"));
        Self {
            surface,
            host,
            _state: state,
        }
    }

    fn post(&mut self, path: &str, body: &Value) -> Response {
        spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: "POST".to_owned(),
                path: path.to_owned(),
                query: String::new(),
                content_type: Some("application/json".to_owned()),
                body: body.to_string().into_bytes(),
            },
        )
        .response
    }

    fn open(&mut self, ticket: &str) -> Response {
        self.post("/tickets", &json!({"ticket_id": ticket}))
    }

    fn touch(&mut self, ticket: &str) -> Response {
        self.post(&format!("/tickets/{ticket}/touches"), &json!({}))
    }

    /// Each event [`Host::published`] lists: the component that ran its command, its name and its
    /// payload.
    fn published(&mut self) -> Vec<(String, String, Value)> {
        self.host
            .published()
            .expect("the published events read")
            .into_iter()
            .map(|published| {
                (
                    published.component,
                    published.event.name,
                    Value::Object(published.event.payload),
                )
            })
            .collect()
    }
}

fn parsed(response: &Response) -> Value {
    serde_json::from_str(&response.body)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", response.body))
}

#[test]
fn a_command_with_an_unpublished_event_is_served() {
    let mut desk = Desk::new(&ir());
    let opened = desk.open("t1");
    assert!(
        !opened.body.contains("does not publish"),
        "the command is refused for an event its component does not publish: {} {}",
        opened.status,
        opened.body
    );
    assert_eq!(opened.status, 201, "{}", opened.body);
    assert_eq!(
        parsed(&opened),
        json!({"outcome": "opened", "published": [{"event": OPENED, "payload": {"ticket_id": "t1"}}]})
    );
}

#[test]
fn an_unpublished_event_is_kept() {
    let ir = ir();
    let mut desk = Desk::new(&ir);
    desk.open("t1");
    assert_eq!(
        desk.published(),
        [(
            DESK.to_owned(),
            OPENED.to_owned(),
            json!({"ticket_id": "t1"})
        )]
    );

    let interpreter = Interpreted::for_model(ir);
    let context = ScenarioContext::new(
        ScenarioId::parse("helpdesk.tickets.OpenTicket/authored/unpublished").expect("a scenario"),
        CorrelationId::new("unpublished").expect("a correlation id"),
    );
    interpreter
        .begin_scenario(&context)
        .expect("the scenario opens");
    let result = interpreter
        .execute_command(SemanticCommandRequest {
            command: OPEN.parse::<CommandRef>().expect("a command"),
            actor: None,
            caller: None,
            input: BTreeMap::from([("ticket_id".to_owned(), Node::Text("t1".to_owned()))]),
            correlation: context.correlation.clone(),
        })
        .expect("ESS's interpreter executes OpenTicket");
    let listed: Vec<(String, BTreeMap<String, Node>)> = result
        .direct_events
        .iter()
        .map(|event| (event.event.to_string(), event.payload.clone()))
        .collect();
    assert_eq!(
        listed,
        [(
            OPENED.to_owned(),
            BTreeMap::from([("ticket_id".to_owned(), Node::Text("t1".to_owned()))])
        )],
        "ESS's interpreter lists the unpublished Opened as the host does"
    );
    let _ = interpreter.end_scenario(&context);
}

#[test]
fn a_published_event_beside_an_unpublished_one_is_sent() {
    let mut desk = Desk::new(&ir());
    desk.open("t1");
    let touched = desk.touch("t1");
    let marks = desk.host.list(MARKS).expect("the marks are listed");
    assert_eq!(
        marks.len(),
        1,
        "the Touched beside the unpublished Noted reaches log: {marks:?}; the touch answered {} {}",
        touched.status,
        touched.body
    );
    assert_eq!(marks[0].get("ticket_id"), Some(&json!("t1")), "{marks:?}");
    assert_eq!(touched.status, 200, "{}", touched.body);
    assert_eq!(parsed(&touched)["outcome"], "touched", "{}", touched.body);
}

#[test]
fn the_unpublished_event_of_a_touch_is_kept() {
    let mut desk = Desk::new(&ir());
    desk.open("t1");
    desk.touch("t1");
    let t1 = json!({"ticket_id": "t1"});
    let published = desk.published();
    let of_desk: Vec<(&str, &Value)> = published
        .iter()
        .filter(|(component, _, _)| component == DESK)
        .map(|(_, event, payload)| (event.as_str(), payload))
        .collect();
    assert_eq!(
        of_desk,
        [(OPENED, &t1), (TOUCHED, &t1), (NOTED, &t1)],
        "{published:?}"
    );
    let names: Vec<&str> = published
        .iter()
        .map(|(_, event, _)| event.as_str())
        .collect();
    assert_eq!(
        names,
        [OPENED, TOUCHED, NOTED, MARKED],
        "the Marked of the delivered Touched follows the desk's events: {published:?}"
    );
}
