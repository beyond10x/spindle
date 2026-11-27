//! Adversary cases of wave 31 U8 (story:dead-letters-kept-and-replayed), round 2: the answers of
//! `GET /api/dead-letters` and `POST /api/dead-letters/replay` driven against the schemas the
//! unit generated (`spindle.observe.DeadLetters`, `spindle.console.ReplayAnswer`), for a replay
//! that is refused with an outcome its binding escalates, one that runs, and one the kernel
//! rejects.
//!
//! The fixture `escalation` ships each placed order through `ship-on-order`, which drops
//! `cancelled`, escalates `no-address` and `oversize` and retries `busy`; the fixture `retried`
//! reaches `index` with a command the kernel does not run.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::console;
use spindle_kernel::External;

const SHIP: &str = "depot.shipping.Ship";
const ORDER: &str = "order-value-90817";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// The errors of `body` against the generated schema `spindle.<name>`.
fn violations(name: &str, body: &Value) -> Vec<String> {
    let path = root().join(format!(
        "ess/generated/jsonschema/schema/types/spindle.{name}.schema.json"
    ));
    let text = std::fs::read_to_string(&path).expect("the schema reads");
    let document: Value = serde_json::from_str(&text).expect("the schema is JSON");
    let validator = jsonschema::draft202012::new(&document).expect("a schema");
    validator
        .iter_errors(body)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect()
}

#[derive(Clone)]
struct Carrier(Arc<Mutex<&'static str>>);

impl Carrier {
    fn answers(&self, outcome: &'static str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = outcome;
    }
}

fn depot(carrier: &Carrier, state: &Path) -> Host {
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/escalation"))
        .expect("the fixture compiles");
    let answering = carrier.clone();
    let decider = move |question: &External<'_>| {
        let outcome = *answering.0.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(question.command == SHIP && question.outcome == outcome)
    };
    Host::deciding(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state,
        Box::new(decider),
    )
    .expect("the host starts")
}

fn listed(host: &mut Host) -> Value {
    let answer = console::dead_letters(host);
    assert_eq!(answer.status, 200, "{:?}", answer.body);
    let body = answer.body.expect("a list");
    assert_eq!(
        violations("observe.DeadLetters", &body),
        Vec::<String>::new()
    );
    body
}

fn replayed(host: &mut Host, component: &str, id: &Value) -> Value {
    let request = json!({"component": component, "id": id}).to_string();
    let (answer, _) = console::replay(host, request.as_bytes());
    assert_eq!(answer.status, 200, "{:?}", answer.body);
    let body = answer.body.expect("an answer");
    assert_eq!(
        violations("console.ReplayAnswer", &body),
        Vec::<String>::new(),
        "{body}"
    );
    body
}

/// A replay refused with an outcome its binding escalates answers `refused` with that outcome
/// and the new dead letter it is kept as; once the carrier ships, a replay of that one answers
/// `ran`, `shipped` and no dead letter. Every answer and list validates against its schema, and
/// no list names the order.
#[test]
fn adv_w31_u8_the_console_answers_a_replays_own_run_as_its_schema_says() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let carrier = Carrier(Arc::new(Mutex::new("busy")));
    let mut host = depot(&carrier, directory.path());
    host.submit(
        "depot.orders.PlaceOrder",
        object(json!({"order_id": ORDER})),
    )
    .expect("the order is placed");
    let list = listed(&mut host);
    assert!(!list.to_string().contains(ORDER), "{list}");
    let first = list["dead_letters"][0].clone();
    assert_eq!(first["attempts"], 5, "{list}");

    carrier.answers("no-address");
    let refused = replayed(&mut host, "shipping", &first["id"]);
    assert_eq!(refused["result"], "refused", "{refused}");
    assert_eq!(refused["outcome"], "no-address", "{refused}");
    assert_eq!(refused["command"], SHIP, "{refused}");
    assert_eq!(refused["id"], first["id"], "{refused}");
    let list = listed(&mut host);
    assert_eq!(
        list["dead_letters"].as_array().map(Vec::len),
        Some(1),
        "{list}"
    );
    let again = list["dead_letters"][0].clone();
    assert_eq!(refused["dead_letter"], again["id"], "{refused} {list}");
    assert_eq!(again["outcome"], "no-address", "{list}");
    assert_eq!(again["attempts"], 1, "{list}");

    carrier.answers("shipped");
    let ran = replayed(&mut host, "shipping", &again["id"]);
    assert_eq!(ran["result"], "ran", "{ran}");
    assert_eq!(ran["outcome"], "shipped", "{ran}");
    assert!(ran.get("dead_letter").is_none(), "{ran}");
    assert_eq!(
        listed(&mut host)["dead_letters"],
        json!([]),
        "the shipped replay is kept nowhere"
    );
    let (answer, _) = console::replay(
        &mut host,
        json!({"component": "shipping", "id": again["id"]})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(answer.status, 404, "{:?}", answer.body);
}

/// A replay the kernel rejects answers `failed`, no outcome, and the new dead letter it is kept
/// as, under the schema.
#[test]
fn adv_w31_u8_a_replay_the_kernel_rejects_answers_failed_with_its_new_letter() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let loaded = spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/retried"))
        .expect("the fixture compiles");
    let mut host = Host::new(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        directory.path(),
    )
    .expect("the host starts");
    host.submit(
        "notes.pad.WriteNote",
        object(json!({"note_id": "n1", "text": "kept"})),
    )
    .expect("the note is written");
    let list = listed(&mut host);
    let first = list["dead_letters"][0].clone();
    let component = first["component"].as_str().expect("a component").to_owned();
    let failed = replayed(&mut host, &component, &first["id"]);
    assert_eq!(failed["result"], "failed", "{failed}");
    assert!(failed.get("outcome").is_none(), "{failed}");
    let list = listed(&mut host);
    assert_eq!(
        failed["dead_letter"], list["dead_letters"][0]["id"],
        "{failed} {list}"
    );
    assert_ne!(failed["dead_letter"], first["id"], "{failed}");
}
