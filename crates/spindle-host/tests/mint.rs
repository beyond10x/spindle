//! The host mints identities only with an implementation it runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-kernel/tests/fixtures/mint")
}

fn environment(mint: &str) -> Environment {
    Environment::parse(&format!(
        "format = \"spindle-environment/1\"\nname = \"mint\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"memory\"]\n\"identity.mint\" = [\"{mint}\"]\n"
    ))
    .expect("the environment parses")
}

fn host(environment: &Environment, state: &Path) -> Result<Host, HostError> {
    let loaded = spindle_spec::load(&fixture()).expect("the mint fixture compiles");
    Host::new(loaded.ir, environment, Arc::new(ManualClock::new()), state)
}

#[test]
fn a_host_mints_a_uuid_v4_identity_where_identity_mint_is_bound_to_uuid_v4() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(&environment("uuid-v4"), state.path()).expect("the host starts");
    let mut input = Map::new();
    input.insert("title".to_owned(), json!("printer jam"));
    let report = host
        .submit("mint.ticket.OpenTicket", input)
        .expect("the ticket opens");
    let identity = report.ran[0].executed.events[0].payload["ticket_id"]
        .as_str()
        .expect("a minted identity")
        .to_owned();
    assert_eq!(identity.len(), 36, "{identity}");
    assert_eq!(&identity[14..15], "4", "{identity} is a UUIDv4");

    let mut close = Map::new();
    close.insert("ticket_id".to_owned(), json!(identity));
    let closed = host
        .submit("mint.ticket.CloseTicket", close)
        .expect("the minted identity addresses the ticket");
    assert_eq!(closed.ran[0].executed.outcome, "closed");
}

#[test]
fn a_host_refuses_identity_mint_bound_to_an_implementation_it_does_not_run() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    match host(&environment("sequence"), state.path()) {
        Err(HostError::Unsupported(why)) => {
            assert!(why.contains("identity.mint"), "{why}");
            assert!(why.contains("sequence"), "{why}");
        }
        Err(other) => panic!("expected Unsupported, got {other}"),
        Ok(_) => panic!("a host that runs no `sequence` mint started"),
    }
}
