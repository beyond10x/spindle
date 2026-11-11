//! `GET /api/served`, the console route that answers every command and view a surface routes and
//! whether it serves it (`story:served-operations-route`), answered by [`spindle_ingress::route`],
//! the router a Service Worker calls, with no listener.
//!
//! The fixture `tests/fixtures/served-operations` declares one component, `pad`, reached over the
//! network: `WriteNote`, which runs, and `DiscardNote`, whose `input_absent:` branch the kernel
//! does not run; `AllNotes` and `NotesWithText`, which the host reads, and one view of each
//! construct the host does not read: `KeptNotes` (a filter it does not read), `NoteCount` (an
//! aggregation), `DoubledCounts` (a computed field), `NotePages` (paging) and `NoteById` (a default
//! row for an absent identity).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use serde_json::Value;
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{HTTP_LOOPBACK, Request, Surface};

const COMPONENT: &str = "pad";
const HEADER: &str = "x-served-caller";
const SECRET: &str = "served-proxy-secret";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("served-operations"))
        .expect("the served-operations fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"served\"\n",
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

fn host(ir: EssIr, state: &Path) -> Host {
    Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state)
        .unwrap_or_else(|error| panic!("the host admits the pad: {error}"))
}

fn get(path: &str) -> Request {
    Request {
        method: "GET".to_owned(),
        path: path.to_owned(),
        query: String::new(),
        content_type: None,
        body: Vec::new(),
    }
}

/// The rows `GET /api/served` answers on `surface`, through the router.
fn served(surface: &Surface, ir: &EssIr) -> Vec<Value> {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = host(ir.clone(), state.path());
    let routed = spindle_ingress::route(surface, &mut host, get("/api/served"));
    assert_eq!(routed.response.status, 200, "{}", routed.response.body);
    let body: Value = serde_json::from_str(&routed.response.body)
        .unwrap_or_else(|error| panic!("{error}: {}", routed.response.body));
    body["operations"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists operations: {body}"))
        .clone()
}

#[test]
fn served_answers_one_row_per_route_with_the_reason_the_start_log_prints() {
    let ir = ir();
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the pad surface is served: {refused}"));
    let rows = served(&surface, &ir);
    assert_eq!(rows.len(), surface.routes().len(), "{rows:#?}");

    let logged: Vec<String> = surface
        .unserved()
        .iter()
        .filter_map(|route| route.not_served())
        .collect();
    let listed: Vec<String> = rows
        .iter()
        .filter(|row| row["served"] == false)
        .map(|row| {
            format!(
                "not served: {} `{}` at {} {}: {}",
                row["kind"].as_str().expect("a kind"),
                row["name"].as_str().expect("a name"),
                row["method"].as_str().expect("a method"),
                row["path"].as_str().expect("a path"),
                row["reason"].as_str().expect("a reason where not served"),
            )
        })
        .collect();
    assert_eq!(
        listed, logged,
        "the rows not served are the start log's lines"
    );

    let named = |served: bool| {
        rows.iter()
            .filter(|row| row["served"] == served)
            .map(|row| {
                format!(
                    "{} {}",
                    row["kind"].as_str().unwrap_or_default(),
                    row["name"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        named(true),
        [
            "command notes.pad.WriteNote",
            "view notes.pad.AllNotes",
            "view notes.pad.NotesWithText",
        ]
    );
    assert_eq!(
        named(false),
        [
            "command notes.pad.DiscardNote",
            "view notes.pad.DoubledCounts",
            "view notes.pad.KeptNotes",
            "view notes.pad.NoteById",
            "view notes.pad.NoteCount",
            "view notes.pad.NotePages",
        ]
    );
    for construct in [
        "`input_absent:`",
        "computed fields",
        "the filter",
        "a default row for an absent identity",
        "an aggregation",
        "paging",
    ] {
        assert!(
            logged.iter().any(|line| line.contains(construct)),
            "one line names {construct}: {logged:#?}"
        );
    }
    for row in rows.iter().filter(|row| row["served"] == true) {
        assert!(
            row.get("reason").is_none(),
            "a served row carries no reason: {row}"
        );
        assert_eq!(row["component"], COMPONENT, "{row}");
    }
}

#[test]
fn served_is_refused_without_the_proxy_secret_as_the_model_is() {
    let ir = ir();
    let surface = Surface::proving_callers(&ir, &[COMPONENT.to_owned()], None, HEADER, SECRET)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the pad surface is served: {refused}"));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = host(ir, state.path());
    let model = spindle_ingress::route(&surface, &mut host, get("/api/model")).response;
    let served = spindle_ingress::route(&surface, &mut host, get("/api/served")).response;
    assert_eq!(model.status, 403, "{}", model.body);
    assert_eq!(
        served, model,
        "GET /api/served is refused as GET /api/model is"
    );
}

#[test]
fn served_is_no_route_without_the_console() {
    let ir = ir();
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .unwrap_or_else(|refused| panic!("the pad surface is served: {refused}"));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = host(ir, state.path());
    let routed = spindle_ingress::route(&surface, &mut host, get("/api/served"));
    assert_eq!(routed.response.status, 404, "{}", routed.response.body);
}
