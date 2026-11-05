//! A stand-in instance for the control process's tests: `spindle-control-fixture <spec>
//! <environment>`.
//!
//! It runs the component in `SPINDLE_COMPONENT` alone on `spindle-host`, on memory stores, and
//! sends its counters to `SPINDLE_TELEMETRY` every `period_ms` (setting, 1000 by default). It
//! links to its peers as `play --component` does: it listens at `SPINDLE_LISTEN` and connects to
//! `SPINDLE_PEER_<PEER>`, and a link that closes later does not end it. With
//! the setting `seed_note` it writes that note first (`notes.pad.WriteNote`). With the setting
//! `ingress` it serves the component's routes there, as `run` serves them. With `exit_after_ms` it
//! exits 0 that long after it started; otherwise it runs until it is signalled, and at most ten
//! minutes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Map, json};
use spindle_host::{Host, SystemClock};
use spindle_telemetry::{Endpoint, Sender, SenderOptions};

/// The longest the fixture runs, so a test that forgets it never leaves it running.
const AT_MOST: Duration = Duration::from_secs(600);
/// How long the fixture waits for a peer to listen.
const LINK_WITHIN: Duration = Duration::from_secs(30);
/// How long one wait for a request takes.
const POLL: Duration = Duration::from_millis(10);

fn setting(name: &str) -> Option<String> {
    std::env::var(format!("SPINDLE_SETTING_{}", name.to_ascii_uppercase())).ok()
}

fn variable(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let spec = args.next().expect("the specification directory");
    let environment = args.next().expect("the environment document");
    let component = variable("SPINDLE_COMPONENT");
    let instance = variable("SPINDLE_INSTANCE_ID");
    let started = Instant::now();

    let loaded = spindle_spec::load(std::path::Path::new(&spec)).expect("the specification");
    let text = std::fs::read_to_string(&environment).expect("the environment");
    let environment = spindle_capability::Environment::parse(&text).expect("an environment");
    let state = tempfile::tempdir().expect("a state directory");
    let ir = loaded.ir.clone();
    let mut host = Host::component(
        loaded.ir,
        &environment,
        Arc::new(SystemClock),
        state.path(),
        &component,
    )
    .unwrap_or_else(|error| panic!("{component}: {error}"));

    let period = setting("period_ms")
        .and_then(|millis| millis.parse().ok())
        .map_or(Duration::from_secs(1), Duration::from_millis);
    let _sender = std::env::var("SPINDLE_TELEMETRY").ok().map(|endpoint| {
        let endpoint: Endpoint = endpoint.parse().expect("a telemetry endpoint");
        let counters = host
            .shared_counters(&component)
            .expect("the component has counters");
        Sender::start(
            counters,
            SenderOptions::new(instance.clone(), endpoint, period),
        )
        .expect("the sender starts")
    });

    let links = std::net::TcpListener::bind(variable("SPINDLE_LISTEN")).expect("SPINDLE_LISTEN");
    for peer in host.peers().into_keys() {
        let name = format!("SPINDLE_PEER_{}", spindle_target::env_name(&peer));
        let address: std::net::SocketAddr = variable(&name).parse().expect("a peer address");
        while host.connect(&peer, address).is_err() {
            assert!(
                started.elapsed() < LINK_WITHIN,
                "{peer} does not listen at {address}"
            );
            std::thread::sleep(POLL);
        }
    }

    if let Some(note) = setting("seed_note") {
        host.submit(
            "notes.pad.WriteNote",
            Map::from_iter([
                ("note_id".to_owned(), json!(note)),
                ("text".to_owned(), json!(format!("written by {instance}"))),
            ]),
        )
        .expect("the seed note is written");
    }

    let listener = setting("ingress").map(|address| {
        let address: std::net::SocketAddr = address.parse().expect("an ingress address");
        let surface = spindle_ingress::Surface::new(&ir, std::slice::from_ref(&component), None)
            .expect("the component's surface");
        spindle_ingress::Listener::bind(address.port(), surface).expect("the ingress listens")
    });
    eprintln!("ready");

    let exit_after = setting("exit_after_ms")
        .and_then(|millis| millis.parse().ok())
        .map(Duration::from_millis);
    while started.elapsed() < AT_MOST {
        if exit_after.is_some_and(|after| started.elapsed() >= after) {
            return;
        }
        let _ = host.accept(&links);
        let _ = host.deliver();
        match &listener {
            Some(listener) => {
                if let Some(exchange) = listener.next(POLL) {
                    exchange.serve(&mut host);
                }
            }
            None => std::thread::sleep(POLL),
        }
    }
}
