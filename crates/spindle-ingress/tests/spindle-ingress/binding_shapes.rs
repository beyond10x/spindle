//! Two `ess-http/1` shapes ESS 0.98.0 refused and ESS 0.99.0 accepts (beyond10x/ess#755), served
//! by [`spindle_ingress::route`] as ESS's generated servers answer them
//! (`story:ess-099-path-types-and-wrapped-views`).
//!
//! A path variable whose source is a `Uuid`, `Boolean`, `Decimal`, `Timestamp` or `Duration`, or
//! an unconstrained newtype of one, beside `String`, `Integer` and enums: the segment is decoded
//! and read as the field's type, as ESS's servers read a `query` value, and a segment the type
//! refuses is answered `400`. A view answered `{<member>: array}` or `{<member>: single}`: the bytes
//! the bare form answers under the one member, the empty answer of a `single` as `empty` declares
//! it.
//!
//! The fixture `tests/fixtures/locker` declares one component, `locker`, reached over the network:
//! `RentLocker` creates a locker holding one value of each such type, `TouchLocker` sets them all,
//! `AllLockers` lists the lockers, and one view per type lists those holding the value given. The
//! views leave out `rent`, a `Decimal`: Spindle writes one as a JSON number where ESS publishes a
//! decimal string (`story:command-input-decimal-text`), which the projected schema would refuse.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Request, Response, Surface};

const COMPONENT: &str = "locker";
const JSON: &str = "application/json";

const FIRST: &str = "6f9619ff-8b86-4d01-b42d-00c04fc964ff";
const SECOND: &str = "0e2a7d4e-3c1b-4f5a-9d8e-7a6b5c4d3e2f";
const SERIAL: &str = "1b4e28ba-2fa1-41d2-883f-0016d3cca427";
const OTHER_SERIAL: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("locker"))
        .expect("the locker fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"binding-shapes\"\n",
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

/// The binding `routes` (YAML, indented under `routes:`) declares, compiled by ESS against `ir`;
/// `Err` with ESS's refusal.
fn compiled(ir: &EssIr, routes: &str) -> Result<ess_gen::http::Binding, String> {
    let text = format!(
        "type: ess-http/1
specification:
  system: locker
  version: v1
  source_digest: sha256:{}
routes:
{routes}",
        ir.source_digest()
    );
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .map_err(|refused| refused.to_string())?;
    ess_gen::http::Binding::new(http).map_err(|refused| format!("not servable: {refused}"))
}

fn binding(ir: &EssIr, routes: &str) -> ess_gen::http::Binding {
    compiled(ir, routes).unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"))
}

fn surface(ir: &EssIr, binding: &ess_gen::http::Binding) -> Surface {
    Surface::new(ir, &[COMPONENT.to_owned()], Some(binding))
        .unwrap_or_else(|refused| panic!("the locker surface is served: {refused}"))
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

fn host(ir: &EssIr, state: &Path) -> Host {
    Host::new(
        ir.clone(),
        &in_memory(),
        Arc::new(ManualClock::new()),
        state,
    )
    .expect("the host admits the locker")
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

/// The `OpenAPI` document ESS projects for the component with the binding.
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

    /// Fails unless `body` validates against the JSON schema the contract declares for the
    /// operation `id` under `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        let schema = paths
            .values()
            .flat_map(|item| item.as_object().expect("a path item is an object").values())
            .find(|operation| operation["operationId"] == id)
            .map(|operation| {
                &operation["responses"][status.to_string()]["content"]["application/json"]["schema"]
            })
            .unwrap_or_else(|| panic!("the contract declares no operation {id}"));
        assert!(
            !schema.is_null(),
            "{id} declares no JSON answer under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{id} answered {body}: {errors:?}");
    }
}

/// `RentLocker` at `POST /lockers`, its input a `json` body.
const RENT: &str = "  - command: locker.cabinet.RentLocker
    method: post
    path: /lockers
    outcomes:
      rented: {status: 201}
";

/// The input `RentLocker` creates the locker `id` with.
fn rented(id: &str) -> Value {
    json!({
        "locker_id": id,
        "code": "front door",
        "open": true,
        "rent": 12.5,
        "since": "2026-11-25T10:00:00Z",
        "term": "PT30M",
        "serial": SERIAL,
        "badge": "b-cd",
    })
}

fn rent(surface: &Surface, host: &mut Host, id: &str) {
    let response = send(surface, host, "POST", "/lockers", Some(rented(id)));
    assert_eq!(response.status, 201, "{}", response.body);
}

#[test]
fn a_newtype_of_uuid_id_is_read_from_the_path_of_a_command_and_a_view_route() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let routes = format!(
        "{RENT}  - command: locker.cabinet.TouchLocker
    method: put
    path: /lockers/{{id}}/touch
    parameters: {{id: input.locker_id}}
    outcomes:
      touched: {{status: 200}}
  - view: locker.cabinet.LockerById
    method: get
    path: /lockers/{{id}}
    parameters: {{id: param.id}}
"
    );
    let bound = binding(&ir, &routes);
    let contract = Contract::of(&ir, &bound);
    let surface = surface(&ir, &bound);
    for operation in ["locker.cabinet.TouchLocker", "locker.cabinet.LockerById"] {
        assert_eq!(not_served(&surface, operation), None, "{operation}");
    }
    let mut host = host(&ir, state.path());
    rent(&surface, &mut host, FIRST);
    rent(&surface, &mut host, SECOND);

    let found = send(
        &surface,
        &mut host,
        "GET",
        &format!("/lockers/{FIRST}"),
        None,
    );
    assert_eq!(found.status, 200, "{}", found.body);
    let body = parsed(&found);
    contract.validate("locker.cabinet.LockerById", 200, &body);
    assert_eq!(body["rows"].as_array().map(Vec::len), Some(1), "{body}");
    assert_eq!(body["rows"][0]["locker_id"], FIRST, "{body}");

    let mut touch = rented(FIRST);
    touch
        .as_object_mut()
        .expect("an object")
        .remove("locker_id");
    touch["code"] = json!("back door");
    let touched = send(
        &surface,
        &mut host,
        "PUT",
        &format!("/lockers/{FIRST}/touch"),
        Some(touch.clone()),
    );
    assert_eq!(touched.status, 200, "{}", touched.body);
    assert_eq!(parsed(&touched)["outcome"], "touched", "{}", touched.body);
    let after = parsed(&send(
        &surface,
        &mut host,
        "GET",
        &format!("/lockers/{FIRST}"),
        None,
    ));
    assert_eq!(after["rows"][0]["code"], "back door", "{after}");
    let other = parsed(&send(
        &surface,
        &mut host,
        "GET",
        &format!("/lockers/{SECOND}"),
        None,
    ));
    assert_eq!(other["rows"][0]["code"], "front door", "{other}");

    for (method, target, body) in [
        ("PUT", "/lockers/not-a-uuid/touch", Some(touch)),
        ("GET", "/lockers/not-a-uuid", None),
    ] {
        let refused = send(&surface, &mut host, method, target, body);
        assert_eq!(refused.status, 400, "{method} {target}: {}", refused.body);
        assert!(
            parsed(&refused)["refused"].is_string(),
            "{method} {target}: {}",
            refused.body
        );
    }
}

/// One type a path segment carries: the input field and view parameter of that type, the view
/// that filters on it, a segment the type reads with the value it reads, and a segment it
/// refuses, where one exists.
struct Carried {
    field: &'static str,
    view: &'static str,
    segment: &'static str,
    value: Value,
    refused: Option<&'static str>,
}

fn carried() -> Vec<Carried> {
    vec![
        Carried {
            field: "code",
            view: "locker.cabinet.LockersByCode",
            segment: "side%20door",
            value: json!("side door"),
            refused: None,
        },
        Carried {
            field: "serial",
            view: "locker.cabinet.LockersBySerial",
            segment: OTHER_SERIAL,
            value: json!(OTHER_SERIAL),
            refused: Some("no-uuid"),
        },
        Carried {
            field: "open",
            view: "locker.cabinet.LockersByOpen",
            segment: "false",
            value: json!(false),
            refused: Some("yes"),
        },
        Carried {
            field: "rent",
            view: "locker.cabinet.LockersByRent",
            segment: "7.25",
            value: json!(7.25),
            refused: Some("cheap"),
        },
        Carried {
            field: "since",
            view: "locker.cabinet.LockersBySince",
            segment: "2026-12-01T08:30:00Z",
            value: json!("2026-12-01T08:30:00Z"),
            refused: Some("yesterday"),
        },
        Carried {
            field: "term",
            view: "locker.cabinet.LockersByTerm",
            segment: "PT45M",
            value: json!("PT45M"),
            // ESS 0.99.0's generated servers read a `Duration` as any text (`json::text_at`), and
            // its interpreter admits any text too: no segment is refused.
            refused: None,
        },
    ]
}

#[test]
fn a_path_variable_of_each_admitted_type_is_read_as_the_type() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut failed = Vec::new();
    for case in carried() {
        let field = case.field;
        let param = field;
        let routes = format!(
            "{RENT}  - command: locker.cabinet.TouchLocker
    method: put
    path: /touch/{{value}}
    parameters: {{value: input.{field}}}
    outcomes:
      touched: {{status: 200}}
  - view: {view}
    method: get
    path: /by/{{value}}
    parameters: {{value: param.{param}}}
",
            view = case.view
        );
        let bound = match compiled(&ir, &routes) {
            Ok(bound) => bound,
            Err(refused) => {
                failed.push(format!("{field}: ESS refuses the binding: {refused}"));
                continue;
            }
        };
        let contract = Contract::of(&ir, &bound);
        let surface = surface(&ir, &bound);
        for operation in ["locker.cabinet.TouchLocker", case.view] {
            if let Some(why) = not_served(&surface, operation) {
                failed.push(format!("{field}: {why}"));
            }
        }
        let mut host = host(&ir, &state.path().join(field));
        rent(&surface, &mut host, FIRST);

        let mut touch = rented(FIRST);
        touch.as_object_mut().expect("an object").remove(field);
        let touched = send(
            &surface,
            &mut host,
            "PUT",
            &format!("/touch/{}", case.segment),
            Some(touch.clone()),
        );
        if touched.status != 200 {
            failed.push(format!(
                "{field}: PUT /touch/{} answered {} {}",
                case.segment, touched.status, touched.body
            ));
        }
        let found = send(
            &surface,
            &mut host,
            "GET",
            &format!("/by/{}", case.segment),
            None,
        );
        if found.status == 200 {
            let body = parsed(&found);
            contract.validate(case.view, 200, &body);
            // `rent` is no view field: the view filtering on it finds the locker only when the
            // touch stored the value the segment spells.
            let row = &body["rows"][0];
            if body["rows"].as_array().map(Vec::len) != Some(1)
                || row["locker_id"] != FIRST
                || (row.get(field).is_some() && row[field] != case.value)
                || (row.get(field).is_none() && field != "rent")
            {
                failed.push(format!(
                    "{field}: GET /by/{} answered {body}, not the locker holding {}",
                    case.segment, case.value
                ));
            }
        } else {
            failed.push(format!(
                "{field}: GET /by/{} answered {} {}",
                case.segment, found.status, found.body
            ));
        }

        if let Some(refused) = case.refused {
            for (method, target, body) in [
                ("PUT", format!("/touch/{refused}"), Some(touch)),
                ("GET", format!("/by/{refused}"), None),
            ] {
                let answer = send(&surface, &mut host, method, &target, body);
                let refusal = serde_json::from_str::<Value>(&answer.body).ok();
                if answer.status != 400
                    || !refusal.is_some_and(|refusal| refusal["refused"].is_string())
                {
                    failed.push(format!(
                        "{field}: {method} {target} answered {} {}, not 400",
                        answer.status, answer.body
                    ));
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn a_constrained_newtype_stays_refused_in_the_path_as_ess_refuses_it() {
    let ir = ir();
    let routes = "  - command: locker.cabinet.TouchLocker
    method: put
    path: /badges/{badge}
    parameters: {badge: input.badge}
    outcomes:
      touched: {status: 200}
";
    let Err(refused) = compiled(&ir, routes) else {
        panic!("ESS admits a newtype with a prefix and an alphabet in the path");
    };
    assert!(refused.contains("ESS-HTTP-009"), "{refused}");
    assert!(
        refused.contains("which a path segment cannot carry"),
        "{refused}"
    );
}

/// The routes of a view answer: `AllLockers` at `GET /lockers` answered `all`, `LockerById` at
/// `GET /lockers/{id}` answered `one`, each a route's `body:` and `empty:` lines.
fn view_routes(all: &str, one: &str) -> String {
    format!(
        "{RENT}  - view: locker.cabinet.AllLockers
    method: get
    path: /lockers
{all}  - view: locker.cabinet.LockerById
    method: get
    path: /lockers/{{id}}
    parameters: {{id: param.id}}
{one}"
    )
}

#[test]
fn a_wrapped_view_answers_its_rows_or_its_one_row_under_its_member() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");

    // The bytes the default `rows` form answers, which every other form is cut from.
    let plain = binding(&ir, &view_routes("", ""));
    let surface_rows = surface(&ir, &plain);
    let mut host_rows = host(&ir, &state.path().join("rows"));
    rent(&surface_rows, &mut host_rows, FIRST);
    rent(&surface_rows, &mut host_rows, SECOND);
    let all_rows = parsed(&send(
        &surface_rows,
        &mut host_rows,
        "GET",
        "/lockers",
        None,
    ))["rows"]
        .to_string();
    let one_row = parsed(&send(
        &surface_rows,
        &mut host_rows,
        "GET",
        &format!("/lockers/{FIRST}"),
        None,
    ))["rows"][0]
        .to_string();

    let mut failed = Vec::new();
    for (name, all, one, all_bytes, one_bytes, empty_status, empty_bytes) in [
        (
            "wrapped",
            "    body: {lockers: array}\n",
            "    body: {locker: single}\n    empty: {status: 404}\n",
            format!("{{\"lockers\":{all_rows}}}"),
            format!("{{\"locker\":{one_row}}}"),
            404,
            String::new(),
        ),
        (
            "wrapped, empty with a literal",
            "    body: {lockers: array}\n",
            "    body: {locker: single}\n    empty: {status: 410, body: {error: {literal: gone}}}\n",
            format!("{{\"lockers\":{all_rows}}}"),
            format!("{{\"locker\":{one_row}}}"),
            410,
            "{\"error\":\"gone\"}".to_owned(),
        ),
        (
            "bare",
            "    body: array\n",
            "    body: single\n    empty: {status: 404}\n",
            all_rows.clone(),
            one_row.clone(),
            404,
            String::new(),
        ),
    ] {
        let bound = match compiled(&ir, &view_routes(all, one)) {
            Ok(bound) => bound,
            Err(refused) => {
                failed.push(format!("{name}: ESS refuses the binding: {refused}"));
                continue;
            }
        };
        let contract = Contract::of(&ir, &bound);
        let surface = surface(&ir, &bound);
        let mut served = true;
        for operation in ["locker.cabinet.AllLockers", "locker.cabinet.LockerById"] {
            if let Some(why) = not_served(&surface, operation) {
                failed.push(format!("{name}: {why}"));
                served = false;
            }
        }
        if !served {
            continue;
        }
        let mut host = host(&ir, &state.path().join(name.replace([' ', ','], "-")));
        rent(&surface, &mut host, FIRST);
        rent(&surface, &mut host, SECOND);
        for (id, target, status, bytes) in [
            (
                "locker.cabinet.AllLockers",
                "/lockers".to_owned(),
                200,
                &all_bytes,
            ),
            (
                "locker.cabinet.LockerById",
                format!("/lockers/{FIRST}"),
                200,
                &one_bytes,
            ),
            (
                "locker.cabinet.LockerById",
                "/lockers/00000000-0000-4000-8000-000000000000".to_owned(),
                empty_status,
                &empty_bytes,
            ),
        ] {
            let answer = send(&surface, &mut host, "GET", &target, None);
            if answer.status != status || answer.body != *bytes {
                failed.push(format!(
                    "{name}: GET {target} answered {} {}, not {status} {bytes}",
                    answer.status, answer.body
                ));
            } else if status == 200 {
                contract.validate(id, status, &parsed(&answer));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
