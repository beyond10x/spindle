//! `[egress]`: an environment binds a published event to an `[http.<name>]` call it declares, and
//! a served run posts each such event there (`story:event-egress-webhook`). The document refuses
//! an egress naming no call; the specification it runs refuses one naming an event no component
//! publishes.

use std::path::{Path, PathBuf};

use spindle_capability::{Egress, Environment, EnvironmentError};

fn relay() -> spindle_spec::EssIr {
    let fixture: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/relay");
    spindle_spec::load(&fixture)
        .expect("the relay fixture compiles")
        .ir
}

fn document(rest: &str) -> String {
    format!(
        "format = \"spindle-environment/1\"\nname = \"test\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"sqlite\"]\n\n{rest}"
    )
}

const HOOK: &str =
    "[http.hook]\nmethod = \"POST\"\nurl = \"http://127.0.0.1:1/events\"\ntimeout_ms = 1000\n";

#[test]
fn an_egress_names_a_published_event_and_its_call() {
    let environment = Environment::parse(&document(&format!(
        "[egress]\n\"relay.source.Pulsed\" = \"hook\"\n\n{HOOK}"
    )))
    .expect("a call named only by an egress is used");
    assert_eq!(
        environment
            .egress_of(&relay())
            .expect("the event is published"),
        vec![Egress {
            event: "relay.source.Pulsed".to_owned(),
            component: "relay-source".to_owned(),
            call: "hook".to_owned(),
        }]
    );
}

#[test]
fn an_egress_naming_no_http_call_is_refused() {
    let refused = Environment::parse(&document("[egress]\n\"relay.source.Pulsed\" = \"hook\"\n"));
    assert!(
        matches!(
            &refused,
            Err(EnvironmentError::EgressCall { event, call })
                if event == "relay.source.Pulsed" && call == "hook"
        ),
        "{refused:?}"
    );
    let message = refused.expect_err("refused").to_string();
    assert!(message.contains("[http.hook]"), "{message}");
}

#[test]
fn an_egress_naming_an_unpublished_event_is_refused() {
    let environment = Environment::parse(&document(&format!(
        "[egress]\n\"relay.source.Pulse\" = \"hook\"\n\n{HOOK}"
    )))
    .expect("the document alone does not know the specification");
    let refused = environment.egress_of(&relay());
    assert!(
        matches!(
            &refused,
            Err(EnvironmentError::EgressUnpublished { event }) if event == "relay.source.Pulse"
        ),
        "a command is no published event: {refused:?}"
    );
}

#[test]
fn an_egress_call_that_is_not_post_is_refused() {
    let refused = Environment::parse(&document(
        "[egress]\n\"relay.source.Pulsed\" = \"hook\"\n\n[http.hook]\nmethod = \"PUT\"\n\
         url = \"http://127.0.0.1:1/events\"\ntimeout_ms = 1000\n",
    ));
    assert!(
        matches!(&refused, Err(EnvironmentError::EgressMethod { call }) if call == "hook"),
        "{refused:?}"
    );
}
