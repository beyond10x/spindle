//! The headerless router on a surface made `Surface::verifying_tokens`
//! (`story:verified-caller-token`): a [`Request`] carries no header, so it carries no bearer
//! token, and every request but the probes is refused `401` before anything is read: the run's own
//! routes under `/__spindle/` answer nothing of what they hold.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, reached over the network.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Caller, CallerToken, Request, Surface, Unverified};

const COMPONENT: &str = "desk";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"router\"\n",
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

/// A verifier that would accept any token it were given: the router must refuse before asking.
struct Accepting;

impl CallerToken for Accepting {
    fn caller(&self, _token: &str) -> Result<Option<Caller>, Unverified> {
        Ok(None)
    }
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

#[test]
fn the_headerless_router_refuses_the_reserved_routes_of_a_token_surface() {
    let ir = ir();
    let surface =
        Surface::verifying_tokens(&ir, &[COMPONENT.to_owned()], None, Arc::new(Accepting))
            .expect("the desk surface verifies tokens")
            .observing(16);
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));

    for path in [
        "/__spindle/metrics",
        "/__spindle/topology",
        "/__spindle/requests",
        "/__spindle/ui",
        "/no/route",
    ] {
        let response = spindle_ingress::route(&surface, &mut host, get(path)).response;
        assert_eq!(response.status, 401, "{path}: {}", response.body);
        assert!(!response.metrics, "{path} is answered as no metrics scrape");
        let body: Value = serde_json::from_str(&response.body)
            .unwrap_or_else(|error| panic!("{path}: {error}: {:?}", response.body));
        assert_eq!(
            body,
            json!({"refused": "unauthenticated"}),
            "{path}: nothing the route holds is answered"
        );
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| name == "www-authenticate" && value == "Bearer"),
            "{path}: {:?}",
            response.headers
        );
    }

    for probe in ["/__spindle/live", "/__spindle/ready", "/__spindle/seed"] {
        let response = spindle_ingress::route(&surface, &mut host, get(probe)).response;
        assert_ne!(
            response.status, 401,
            "{probe} answers without a token: {}",
            response.body
        );
    }
}
