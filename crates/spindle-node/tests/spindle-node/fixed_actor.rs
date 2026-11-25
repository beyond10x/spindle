//! Fixed actors (`story:node-export-fixed-actor`): the loopback listener serves the fixed-callers
//! fixture of `spindle-cli` with the clerk, then the keeper, fixed as
//! `run --fixed-actor bureau.desk.Clerk --fixed-caller {"badge": "ada", "shift": "early"}
//! --fixed-actor bureau.desk.Keeper --fixed-caller {"key": "brass", "shift": "late"}` fixes them
//! ([`Caller::fixed`], [`Surface::fixing_callers`]), and answers the exchanges
//! `tests/fixtures/fixed-actor-exchanges.json` records. The Node export's tests replay the same
//! record. The opening both wasm hosts call refuses a fixed actor as `run` does.

use std::sync::Arc;

use serde_json::json;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Caller, HTTP_LOOPBACK, Listener, Surface};
use spindle_spec::EssIr;

use crate::listener::{Exchange, listened, loopback};

const SOURCES: [(&str, &str); 2] = [
    (
        "system.yaml",
        include_str!("../../../spindle-cli/tests/fixtures/fixed-callers/system.yaml"),
    ),
    (
        "domains/desk.yaml",
        include_str!("../../../spindle-cli/tests/fixtures/fixed-callers/domains/desk.yaml"),
    ),
];

const NODE: &str = include_str!("../../../../examples/environments/node.toml");

const RECORD: &str = include_str!("../fixtures/fixed-actor-exchanges.json");

fn sources() -> Vec<(String, String)> {
    SOURCES
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect()
}

fn ir() -> EssIr {
    let (names, texts): (Vec<String>, Vec<String>) = sources().into_iter().unzip();
    spindle_spec::compile(&names, &texts).expect("the fixed-callers fixture compiles")
}

fn caller(actor: &str, attributes: serde_json::Value) -> Caller {
    let serde_json::Value::Object(attributes) = attributes else {
        panic!("caller attributes are an object");
    };
    Caller {
        actor: actor.to_owned(),
        attributes,
    }
}

/// The clerk, then the keeper, as the Node export's test fixes them.
fn clerk_then_keeper() -> Vec<Caller> {
    vec![
        caller(
            "bureau.desk.Clerk",
            json!({"badge": "ada", "shift": "early"}),
        ),
        caller(
            "bureau.desk.Keeper",
            json!({"key": "brass", "shift": "late"}),
        ),
    ]
}

#[test]
fn the_loopback_listener_answers_the_fixed_actor_exchanges() {
    let ir = ir();
    let callers = clerk_then_keeper();
    for fixed in &callers {
        fixed
            .fixed(&ir)
            .unwrap_or_else(|refused| panic!("{} is fixed: {refused}", fixed.actor));
    }
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(
        ir.clone(),
        &loopback(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .unwrap_or_else(|error| panic!("the host admits the fixed-callers fixture: {error}"));
    let surface = Surface::fixing_callers(&ir, &["office".to_owned()], None, &callers)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the office surface is served: {refused}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");

    let recorded: Vec<Exchange> = serde_json::from_str(RECORD).expect("the record reads");
    let listened = listened(&listener, &recorded, |served| {
        served.serve(&mut host);
    });
    assert_eq!(
        listened,
        recorded,
        "the listener answers what tests/fixtures/fixed-actor-exchanges.json records; it wrote:\n{}",
        serde_json::to_string_pretty(&listened).unwrap_or_default()
    );
}

/// What the opening of `spindle-embed` answers for the fixed-callers fixture on the Node
/// environment with `callers` fixed: `Ok(())` where it opens, the refusal otherwise.
fn opening(callers: Vec<Caller>) -> Result<(), String> {
    let admitted = spindle_embed::admit(sources(), NODE)?;
    admitted
        .open(spindle_embed::Opening {
            stores: Stores::new(),
            reached: vec!["office".to_owned()],
            decisions: None,
            answer_default: false,
            binding: None,
            callers,
            worlds: true,
            clock: Arc::new(ManualClock::new()),
        })
        .map(drop)
}

#[test]
fn the_opening_refuses_a_fixed_actor_as_run_does() {
    assert_eq!(opening(clerk_then_keeper()), Ok(()));
    assert_eq!(
        opening(vec![caller("bureau.desk.Nobody", json!({}))]),
        Err(
            "the fixed actor bureau.desk.Nobody: the specification declares no such actor"
                .to_owned()
        ),
        "an undeclared actor, with the reason run gives for --fixed-actor"
    );
    let mut twice = clerk_then_keeper();
    twice.push(caller(
        "bureau.desk.Clerk",
        json!({"badge": "bob", "shift": "late"}),
    ));
    assert_eq!(
        opening(twice),
        Err(
            "the fixed actor bureau.desk.Clerk is given twice; the first would always be sent as"
                .to_owned()
        ),
    );
    let unbound = opening(vec![caller("bureau.desk.Clerk", json!({"badge": "ada"}))])
        .expect_err("a clerk without the shift its binding requires");
    assert!(
        unbound.starts_with(
            "the caller of the fixed actor bureau.desk.Clerk: invalid input: the caller is not a \
             caller of `bureau.desk.Clerk`: "
        ),
        "the kernel's refusal, as run gives it for --fixed-caller: {unbound}"
    );
}
