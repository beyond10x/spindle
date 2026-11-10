//! `spindle.console.World` (`ess/domains/console.yaml`, `story:store-reset-and-world-specified`)
//! says: "`play --component` serves no console route, so every reset there that passes its token
//! and `Host` checks is answered `404`".
//!
//! `play --component` serves its ingress from `Surface::new(ir, &[component], None)`, no console
//! route, and routes every request with [`spindle_ingress::route`], no world reader
//! (`crates/spindle-cli/src/component.rs:258`, `crates/spindle-ingress/src/listener.rs:103`).
//! This case builds that surface on the desk fixture and sends `POST /api/reset` with a world and
//! without one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Request, Surface};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"component\"\n",
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

#[test]
fn a_component_ingress_answers_every_reset_404() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &["desk".to_owned()], None).expect("the desk surface");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .expect("the host admits the desk");
    let reset = |body: &[u8]| Request {
        method: "POST".to_owned(),
        path: "/api/reset".to_owned(),
        query: String::new(),
        content_type: Some("application/json".to_owned()),
        body: body.to_vec(),
    };
    for body in [&br#"{"mint":7}"#[..], b""] {
        let routed = spindle_ingress::route(&surface, &mut host, reset(body));
        assert_eq!(
            routed.response.status,
            404,
            "spindle.console.World: play --component answers every reset 404; {:?} answered {} {}",
            String::from_utf8_lossy(body),
            routed.response.status,
            routed.response.body
        );
    }

    // The browser run's surface, the same routes with the console beside them and still no world
    // reader: a reset with a body is refused 400, one without is answered 200.
    let console = surface
        .with_console(spindle_ingress::HTTP_LOOPBACK)
        .expect("the console routes");
    let routed = spindle_ingress::route(&console, &mut host, reset(br#"{"mint":7}"#));
    assert_eq!(
        routed.response.status, 400,
        "spindle.console.World: the browser run refuses a reset with a body 400; answered {} {}",
        routed.response.status, routed.response.body
    );
    let routed = spindle_ingress::route(&console, &mut host, reset(b""));
    assert_eq!(
        routed.response.status, 200,
        "spindle.console.ResetAnswer: a reset without a body is answered 200; answered {} {}",
        routed.response.status, routed.response.body
    );
}
