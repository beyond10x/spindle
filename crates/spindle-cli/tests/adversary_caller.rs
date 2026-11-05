//! Adversary case for story:kernel-caller-attributes: the HTTP surface of the commands that read
//! the caller. The specification is `crates/spindle-conformance/tests/fixtures/adversary-caller`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SPEC: &str = "crates/spindle-conformance/tests/fixtures/adversary-caller";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `Veto` reads `caller.owner_id` in the `when_subject:` of an `external:` branch, and `Lock`
/// reads `{caller: owner_id}` in its error's payload: both read the caller, and how an HTTP
/// caller proves its attributes is not decided, so both stay unserved like `Claim`, `Hand` and
/// `Keep`. `Ping` reads nothing of the caller and is not unserved for it. The ingress takes the
/// read set from `ess_gen::http::caller_attributes`, which reads neither an `ExternalSubject`
/// guard nor an error payload.
#[test]
fn every_route_whose_command_reads_the_caller_stays_unserved_and_only_those() {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the adversary caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["desks".to_owned()], None)
        .expect("the fixture's surface is routed");
    let routes: BTreeMap<String, String> = surface
        .routes()
        .iter()
        .map(|route| {
            (
                route.operation.to_string(),
                route
                    .unserved()
                    .map_or_else(|| "served".to_owned(), ToString::to_string),
            )
        })
        .collect();
    let reason = |operation: &str| {
        routes
            .iter()
            .find(|(name, _)| name.to_lowercase().contains(operation))
            .map(|(_, reason)| reason.clone())
            .unwrap_or_else(|| panic!("a route for {operation}: {routes:#?}"))
    };
    assert!(
        !reason("ping").contains("caller"),
        "Ping reads no caller: {routes:#?}"
    );
    for operation in ["claim", "hand", "keep", "veto", "lock"] {
        assert!(
            reason(operation).contains("caller attributes it reads"),
            "{operation} reads the caller and stays unserved: {routes:#?}"
        );
    }
}
