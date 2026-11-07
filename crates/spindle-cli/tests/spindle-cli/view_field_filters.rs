//! `run` serving views whose filter reads a field of the source's own row: compared with a literal
//! of its type (`archived == false`, `kind != "draft"`, `priority >= 3`), listed `in:` or
//! `none_of:` literals, guarded by `defined`, or quantified over a `List<String>` field with
//! `exists` or `forall` (`story:view-field-literal-filters`).
//!
//! `tests/fixtures/view-field-filters` declares one component, `tray`, reached over the network:
//! `FileCard` files a card with a Boolean `archived`, an Integer `priority`, a String `kind`, an
//! `Optional<String>` `note` and a `List<String>` `tags`, and each view filters the cards by one
//! construct. Every route is read from the `OpenAPI` document ESS projects for the component,
//! every `200` answer is validated against its schema, and every set of rows is compared with the
//! rows ESS's interpreter answers over the same cards.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/view-field-filters";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "tray";

const FILE_CARD: &str = "cards.tray.FileCard";
const CURRENT: &str = "cards.tray.Current";
const FINAL: &str = "cards.tray.Final";
const URGENT: &str = "cards.tray.Urgent";
const MIDDLING: &str = "cards.tray.Middling";
const UNFINISHED: &str = "cards.tray.Unfinished";
const SETTLED: &str = "cards.tray.Settled";
const KEPT: &str = "cards.tray.Kept";
const NOTED: &str = "cards.tray.Noted";
const NOTED_KEEP: &str = "cards.tray.NotedKeep";
const FLAGGED: &str = "cards.tray.Flagged";
const FRESH: &str = "cards.tray.Fresh";
const TAGGED_WITH: &str = "cards.tray.TaggedWith";
const TAGGED_ANY: &str = "cards.tray.TaggedAny";
/// The view whose filter mixes an admitted leaf with one still unread (`starts_with`).
const DRAFT_LIKE: &str = "cards.tray.DraftLike";

/// Every view read with each set of parameters it is read with here: the query string and the
/// same values as ESS parameters. A view without parameters is read once, with none.
const READS: [(&str, &str, &str); 17] = [
    (CURRENT, "", "{}"),
    (FINAL, "", "{}"),
    (URGENT, "", "{}"),
    (MIDDLING, "", "{}"),
    (UNFINISHED, "", "{}"),
    (SETTLED, "", "{}"),
    (KEPT, "", "{}"),
    (NOTED, "", "{}"),
    (NOTED_KEEP, "", "{}"),
    (FLAGGED, "", "{}"),
    (FRESH, "", "{}"),
    (TAGGED_WITH, "wanted=stale", r#"{"wanted": "stale"}"#),
    (TAGGED_WITH, "wanted=later", r#"{"wanted": "later"}"#),
    (TAGGED_WITH, "wanted=none", r#"{"wanted": "none"}"#),
    (
        TAGGED_ANY,
        "wanted=later&wanted=urgent",
        r#"{"wanted": ["later", "urgent"]}"#,
    ),
    (TAGGED_ANY, "wanted=stale", r#"{"wanted": ["stale"]}"#),
    (DRAFT_LIKE, "", "{}"),
];

/// The cards every test files. `c1` and `c5` hold no note; `c2` holds no tag.
fn cards() -> Vec<Value> {
    vec![
        json!({"card_id": "c1", "archived": false, "priority": 1, "kind": "draft",
               "tags": ["urgent"]}),
        json!({"card_id": "c2", "archived": true, "priority": 3, "kind": "final",
               "note": "keep", "tags": []}),
        json!({"card_id": "c3", "archived": false, "priority": 5, "kind": "review",
               "note": "toss", "tags": ["urgent", "stale"]}),
        json!({"card_id": "c4", "archived": false, "priority": 2, "kind": "final",
               "note": "keep", "tags": ["stale"]}),
        json!({"card_id": "c5", "archived": true, "priority": 4, "kind": "draft",
               "tags": ["later"]}),
    ]
}

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for `tray`.
struct Contract(Value);

impl Contract {
    fn of(ir: &ess_compiler::EssIr) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::json(ir, component);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method and path of `id`.
    fn operation(&self, id: &str) -> (String, String) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone());
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    /// Fails unless `body` validates against the schema the contract declares for `id`'s answer
    /// under `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.operation(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
}

/// A running `b10x-spindle run` serving the fixture over HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = match receiver.recv_timeout(STARTUP) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run printed no line within {STARTUP:?}: {error}");
        }
    };
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        _state: state,
    }
}

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, target: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// `run` serving the fixture with [`cards`] filed, and ESS's interpreter with the same commands
/// run on it.
struct Tray {
    served: Served,
    contract: Contract,
    interpreter: Interpreted,
    views: Vec<String>,
}

impl Tray {
    fn open() -> Self {
        let ir = spindle_spec::load(&root().join(SPEC))
            .expect("the fixture compiles")
            .ir;
        let contract = Contract::of(&ir);
        let views = ir.views().keys().map(ToString::to_string).collect();
        let interpreter = Interpreted::for_model(ir);
        let served = serve();
        let correlation = CorrelationId::new("view-field-filters").expect("a correlation");
        let (method, path) = contract.operation(FILE_CARD);
        for input in cards() {
            let (status, answer) = exchange(served.address, &method, &path, Some(&input));
            assert_eq!(status, 202, "{FILE_CARD} {input}: {answer}");
            assert_eq!(answer["outcome"], "filed", "{answer}");
            let input: BTreeMap<String, Node> =
                serde_json::from_value(input).expect("the input as ESS values");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: FILE_CARD.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{FILE_CARD} runs on the interpreter: {error:?}"));
        }
        Self {
            served,
            contract,
            interpreter,
            views,
        }
    }

    /// `view` read with the query string `query` (none where empty): the status and the body.
    fn read(&self, view: &str, query: &str) -> (u16, Value) {
        let (method, path) = self.contract.operation(view);
        let target = if query.is_empty() {
            path
        } else {
            format!("{path}?{query}")
        };
        exchange(self.served.address, &method, &target, None)
    }

    /// `view` read with `query`, answered `200` with a body its schema admits: the body.
    fn rows(&self, view: &str, query: &str) -> Value {
        let (status, body) = self.read(view, query);
        assert_eq!(status, 200, "{view}?{query}: {body}");
        self.contract.validate(view, status, &body);
        body
    }

    /// The rows ESS's interpreter answers for `view` read with `params`, as JSON, or `None` where
    /// it refuses the read: the model does not determine its rows.
    fn interpreted(&self, view: &str, params: &str) -> Option<Value> {
        let params: BTreeMap<String, Node> =
            serde_json::from_str(params).expect("the parameters as ESS values");
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("view-field-filters").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .ok()?;
        Some(json!({
            "rows": absent(integral(serde_json::to_value(answer.rows).expect("rows as JSON")))
        }))
    }

    /// Fails unless `view` read with `query` answers `200` with exactly the card ids `expected`,
    /// and exactly the rows ESS's interpreter answers for `params`.
    fn agrees(&self, view: &str, query: &str, params: &str, expected: &[&str]) {
        let body = self.rows(view, query);
        assert_eq!(ids(&body), expected, "{view}?{query}: {body}");
        assert_eq!(
            Some(&body),
            self.interpreted(view, params).as_ref(),
            "{view}?{query}: the rows ESS's interpreter answers"
        );
    }
}

/// `value` with every integral float written as the integer it is: ESS's values reach JSON here
/// through `f64`, so the interpreter's `1` arrives as `1.0`, which JSON compares unequal to the
/// `1` a served answer writes.
fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if number.is_f64() && float.fract() == 0.0 && float.abs() < 9.0e15 => {
                json!(float as i64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(members) => Value::Object(
            members
                .into_iter()
                .map(|(name, member)| (name, integral(member)))
                .collect(),
        ),
        other => other,
    }
}

/// `rows`, each row without the members that are `null`: the interpreter's semantic row holds an
/// `Optional` field the card does not carry as `null`, and the answer on the wire leaves it out,
/// as a field without a presence policy is answered (`project` in `spindle-host`) and as the
/// projected schema admits.
fn absent(rows: Value) -> Value {
    match rows {
        Value::Array(rows) => Value::Array(
            rows.into_iter()
                .map(|row| match row {
                    Value::Object(members) => Value::Object(
                        members
                            .into_iter()
                            .filter(|(_, member)| !member.is_null())
                            .collect(),
                    ),
                    other => other,
                })
                .collect(),
        ),
        other => other,
    }
}

/// The card ids of `body`'s rows, in the order answered.
fn ids(body: &Value) -> Vec<&str> {
    body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("an answer with rows: {body}"))
        .iter()
        .map(|row| row["card_id"].as_str().expect("a card id"))
        .collect()
}

/// The text of a `501` refusal, failing unless the answer is one.
fn refused(status: u16, answer: &Value) -> &str {
    assert_eq!(status, 501, "{answer}");
    answer["refused"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal in words: {answer}"))
}

#[test]
fn a_field_compared_with_a_literal_filters_rows() {
    let tray = Tray::open();
    tray.agrees(CURRENT, "", "{}", &["c1", "c3", "c4"]);
    tray.agrees(FINAL, "", "{}", &["c2", "c3", "c4"]);
    tray.agrees(URGENT, "", "{}", &["c2", "c3", "c5"]);
    tray.agrees(MIDDLING, "", "{}", &["c2", "c4", "c5"]);
}

#[test]
fn a_field_listed_among_literals_filters_rows() {
    let tray = Tray::open();
    tray.agrees(UNFINISHED, "", "{}", &["c1", "c3", "c5"]);
    tray.agrees(SETTLED, "", "{}", &["c2", "c3", "c4"]);
}

#[test]
fn an_absent_optional_field_is_decided_as_ess_decides_it() {
    let tray = Tray::open();
    tray.agrees(KEPT, "", "{}", &["c1", "c2", "c4", "c5"]);
    tray.agrees(NOTED, "", "{}", &["c3"]);
    assert_eq!(
        tray.interpreted(NOTED_KEEP, "{}"),
        None,
        "ESS's interpreter leaves `note == \"keep\"` unknown for a card without a note, and \
         refuses the read"
    );
    let (status, answer) = tray.read(NOTED_KEEP, "");
    let text = refused(status, &answer);
    assert!(
        text.contains("note == keep") || text.contains("note == \"keep\""),
        "the refusal names the filter it cannot decide: {text}"
    );
}

#[test]
fn a_quantifier_over_a_list_field_filters_rows() {
    let tray = Tray::open();
    tray.agrees(FLAGGED, "", "{}", &["c1", "c3"]);
    tray.agrees(FRESH, "", "{}", &["c1", "c2", "c5"]);
    tray.agrees(
        TAGGED_WITH,
        "wanted=stale",
        r#"{"wanted": "stale"}"#,
        &["c3", "c4"],
    );
    tray.agrees(
        TAGGED_WITH,
        "wanted=later",
        r#"{"wanted": "later"}"#,
        &["c5"],
    );
    tray.agrees(
        TAGGED_ANY,
        "wanted=later&wanted=urgent",
        r#"{"wanted": ["later", "urgent"]}"#,
        &["c1", "c3", "c5"],
    );
}

#[test]
fn an_unread_filter_is_still_refused_whole() {
    let tray = Tray::open();
    let (status, answer) = tray.read(DRAFT_LIKE, "");
    let text = refused(status, &answer);
    assert!(
        text.contains(DRAFT_LIKE) && text.contains("starts_with"),
        "the refusal names the view and its whole filter: {text}"
    );
    assert!(
        text.contains("archived == false"),
        "the admitted leaf is refused with the rest, not evaluated alone: {text}"
    );
}

#[test]
fn field_filters_agree_with_the_interpreter() {
    let tray = Tray::open();
    let read = READS.iter().map(|(view, ..)| *view).collect::<Vec<_>>();
    for view in &tray.views {
        assert!(
            read.contains(&view.as_str()),
            "every view of the fixture is read here; {view} is not"
        );
    }
    for (view, query, params) in READS {
        let (status, body) = tray.read(view, query);
        if view == DRAFT_LIKE {
            refused(status, &body);
            continue;
        }
        match tray.interpreted(view, params) {
            Some(rows) => {
                assert_eq!(status, 200, "{view}?{query}: {body}");
                tray.contract.validate(view, status, &body);
                assert_eq!(
                    body, rows,
                    "{view}?{query}: the rows ESS's interpreter answers"
                );
            }
            None => {
                refused(status, &body);
            }
        }
    }
}
