//! `story:events-record-their-caller`: an event a seed entry committed records the seed as its
//! cause, with the entry's actor, its caller and its `request_id`, read back from the state's
//! store after the run.

use serde_json::{Map, json};
use spindle_kernel::{Provenance, Sender, SqliteStore, Store};

use super::{OPERATOR, environment, grant, scratch, serve, write};

#[test]
fn a_seed_event_records_the_seed() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state_text = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-entry-1")]).to_string(),
    );
    let document = environment(&directory, "sqlite", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--state",
        state_text,
        "--seed",
        &seed,
        "--answer-default",
    ]);
    let ended = served.ended();
    assert!(ended.contains("1 sent, 1 applied"), "{ended}");
    served.stop();

    let mut store = SqliteStore::open(&state.join("gate.sqlite")).expect("the gate's state opens");
    let committed = store.events_after(0, usize::MAX).expect("the events read");
    assert!(!committed.is_empty(), "the entry committed an event");
    for committed in &committed {
        let origin = committed.origin.as_ref().expect("an origin");
        assert_eq!(origin.command, "vault.access.GrantRole");
        assert_eq!(
            origin.cause,
            Some(Provenance::Seed(Sender {
                actor: Some(OPERATOR.to_owned()),
                attributes: Map::from_iter([("tenant".to_owned(), json!("north"))]),
                request: Some("seed-entry-1".to_owned()),
            })),
            "{committed:?}"
        );
    }
}
