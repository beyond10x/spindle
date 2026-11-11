//! `GET /api/events?after=<cursor>&limit=<n>` (`story:events-range-read`): at most `n` events
//! after the cursor, in the order `GET /api/events` lists them, with the cursor of the last one
//! as `next`; `limit` 100 by default and 1,000 at most; a malformed `after` or `limit` refused
//! `400`; without a query, the answer as before. The request log records a console request's
//! query string, bounded as its path is, every value but the cursor and the limit written `…`.
//!
//! The console fixture declares two components: `board`, which a request reaches, and `ledger`,
//! which only a binding reaches.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Answer, HTTP_LOOPBACK, Listener, Request, Surface, console, route};
use spindle_kernel::{Event, MemoryStore, Origin, Store};

/// The default `limit`, as `website/docs` states it.
const DEFAULT: usize = 100;
/// The largest `limit`, as `website/docs` states it.
const MOST: usize = 1_000;
/// The most bytes of a record's path, and of its query.
const PATH_MOST: usize = 2_048;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tasks() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/ingress"))
        .expect("the console fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn bound(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"range-{store}\"\n",
            "os = \"linux\"\n",
            "arch = \"x86_64\"\n",
            "\n",
            "[provides]\n",
            "\"store.durable\" = [\"{store}\"]\n",
            "\"store.ephemeral\" = [\"{store}\"]\n",
            "clock = [\"system\"]\n",
            "timer = [\"tokio\"]\n",
            "channel = [\"in-process\"]\n",
            "projection = [\"in-process\"]\n",
            "\"ingress.network\" = [\"http-loopback\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

fn surface(ir: &ess_compiler::EssIr) -> Surface {
    Surface::new(ir, &["board".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the board surface is served: {refused}"))
}

fn submit(host: &mut Host, command: &str, input: Value) {
    let report = host
        .submit(command, object(input))
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

/// A host on memory stores with two tasks opened and the first closed: four events.
fn scripted() -> Host {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(
        tasks(),
        &bound("memory"),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the memory host opens");
    for (id, title) in [("t1", "First"), ("t2", "Second")] {
        submit(
            &mut host,
            "tasks.board.OpenTask",
            json!({"task_id": id, "title": title}),
        );
    }
    submit(&mut host, "tasks.board.CloseTask", json!({"task_id": "t1"}));
    host
}

/// A host whose board's store holds `events` events already.
fn long(events: usize) -> Host {
    let mut board = MemoryStore::default();
    for index in 0..events {
        board.attribute(Some(Origin {
            command: "tasks.board.OpenTask".to_owned(),
            sequence: u64::try_from(index).expect("an index fits") + 1,
        }));
        board
            .commit_events(
                &[Event {
                    name: "tasks.board.TaskOpened".to_owned(),
                    payload: object(json!({"task_id": format!("t{index}"), "title": "Old"})),
                }],
                &[],
            )
            .expect("the board records the event");
    }
    board.attribute(None);
    let given: Stores = [
        ("board".to_owned(), Box::new(board) as Box<dyn Store>),
        (
            "ledger".to_owned(),
            Box::new(MemoryStore::default()) as Box<dyn Store>,
        ),
    ]
    .into_iter()
    .collect();
    Host::supplied(
        tasks(),
        &bound("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        given,
    )
    .expect("the host on given stores opens")
}

/// `GET /api/events?<query>` routed on `host`: the status and the body.
fn routed(host: &mut Host, query: &str) -> (u16, Value) {
    let ir = host.ir().clone();
    let response = route(
        &surface(&ir),
        host,
        Request {
            method: "GET".to_owned(),
            path: "/api/events".to_owned(),
            query: query.to_owned(),
            content_type: None,
            body: Vec::new(),
        },
    )
    .response;
    let body = serde_json::from_str(&response.body)
        .unwrap_or_else(|error| panic!("{error}: {}", response.body));
    (response.status, body)
}

/// `GET /api/events?<query>` answered beside the host's thread, through its reader.
fn beside(host: &Host, query: &str) -> (u16, Value) {
    let reader = host.reader();
    let query = query.to_owned();
    let answer: Answer = std::thread::spawn(move || console::events_read_queried(&reader, &query))
        .join()
        .expect("the read beside ends");
    (answer.status, answer.body.unwrap_or(Value::Null))
}

fn events(body: &Value) -> Vec<Value> {
    body["events"]
        .as_array()
        .unwrap_or_else(|| panic!("{{\"events\": [...]}}: {body}"))
        .clone()
}

fn next(body: &Value) -> String {
    body["next"]
        .as_str()
        .unwrap_or_else(|| panic!("a next cursor: {body}"))
        .to_owned()
}

/// Acceptance 1 and 3: each page is at most `limit` events after the cursor, in the order the
/// whole list has them, with `next`; at the end the page is empty and `next` is the cursor sent;
/// beside the host's thread alike.
#[test]
fn a_range_answers_at_most_limit_events_after_the_cursor() {
    let mut host = scripted();
    let (status, whole) = routed(&mut host, "");
    assert_eq!(status, 200, "{whole}");
    let whole = events(&whole);
    assert_eq!(whole.len(), 4, "{whole:?}");

    let (status, first) = routed(&mut host, "limit=3");
    assert_eq!(status, 200, "{first}");
    assert_eq!(events(&first), whole[..3]);
    let cursor = next(&first);
    let (status, rest) = routed(&mut host, &format!("after={cursor}&limit=3"));
    assert_eq!(status, 200, "{rest}");
    assert_eq!(events(&rest), whole[3..]);
    let (status, read) = beside(&host, &format!("after={cursor}&limit=3"));
    assert_eq!(status, 200, "{read}");
    assert_eq!(read, rest, "beside the host's thread alike");

    let end = next(&rest);
    for query in [format!("after={end}"), format!("limit=7&after={end}")] {
        let (status, idle) = routed(&mut host, &query);
        assert_eq!(status, 200, "{idle}");
        assert_eq!(idle, json!({"events": [], "next": end}), "{query}");
        let (status, idle) = beside(&host, &query);
        assert_eq!(status, 200, "{idle}");
        assert_eq!(idle, json!({"events": [], "next": end}), "{query}, beside");
    }

    submit(
        &mut host,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    let (status, new) = routed(&mut host, &format!("after={end}"));
    assert_eq!(status, 200, "{new}");
    assert_eq!(events(&new).len(), 1, "{new}");
    assert_eq!(events(&new)[0]["payload"]["task_id"], "t3");
}

/// Acceptance 2: `limit` is 100 by default and 1,000 at most; a larger one is answered with
/// 1,000.
#[test]
fn the_limit_has_a_default_and_a_maximum() {
    let mut host = long(MOST + 500);
    let (status, page) = routed(&mut host, "limit=5000");
    assert_eq!(status, 200);
    assert_eq!(
        events(&page).len(),
        MOST,
        "a larger limit is answered with the maximum"
    );
    let (status, page) = routed(&mut host, "limit=184467440737095516150000");
    assert_eq!(status, 200);
    assert_eq!(
        events(&page).len(),
        MOST,
        "a limit past any integer is the maximum"
    );
    let cursor = next(&page);
    let (status, page) = routed(&mut host, &format!("after={cursor}"));
    assert_eq!(status, 200);
    assert_eq!(events(&page).len(), DEFAULT, "the default limit");
    let (status, page) = routed(&mut host, &format!("after={cursor}&limit={MOST}"));
    assert_eq!(status, 200);
    assert_eq!(events(&page).len(), 500, "the rest");
}

/// Acceptance 2: a malformed `after` or `limit` is refused `400`, on the host's thread and
/// beside it, naming the parameter and never echoing the value.
#[test]
fn a_malformed_after_or_limit_is_refused() {
    let mut host = scripted();
    let (_, page) = routed(&mut host, "limit=1");
    let cursor = next(&page);
    // A run of another system, whose one component is `tower`: its cursor marks no component
    // this run has.
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let beacons =
        spindle_spec::load(&root().join("crates/spindle-ingress/tests/fixtures/unwritten"))
            .expect("the unwritten fixture compiles")
            .ir;
    let mut other = Host::new(
        beacons,
        &bound("memory"),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the beacons host opens");
    let foreign = other
        .published_after(&spindle_host::Cursor::start(), 1)
        .expect("the beacons host reads")
        .next
        .to_string();
    let mut forged = cursor.clone();
    forged.push('A');
    let malformed = [
        ("after=", "after"),
        ("after=not-a-cursor", "after"),
        ("after=%FF", "after"),
        (&format!("after={forged}"), "after"),
        (&format!("after={cursor}&after={cursor}"), "after"),
        ("limit=", "limit"),
        ("limit=0", "limit"),
        ("limit=-1", "limit"),
        ("limit=1.5", "limit"),
        ("limit=ten", "limit"),
        ("limit=%201", "limit"),
        ("limit=1&limit=2", "limit"),
    ];
    for (query, parameter) in malformed {
        for (place, (status, body)) in [
            ("on the host's thread", routed(&mut host, query)),
            ("beside", beside(&host, query)),
        ] {
            assert_eq!(status, 400, "{query} {place}: {body}");
            let refused = body["refused"]
                .as_str()
                .unwrap_or_else(|| panic!("{query}: {{\"refused\"}}: {body}"));
            assert!(
                refused.contains(&format!("query.{parameter}")),
                "{query}: names the parameter: {refused}"
            );
            assert!(
                !refused.contains("not-a-cursor") && !refused.contains("ten"),
                "{query}: the value is not echoed: {refused}"
            );
        }
    }
    let (status, body) = routed(&mut host, &format!("after={foreign}"));
    assert_eq!(
        status, 400,
        "a cursor of another run's components is refused: {body}"
    );
}

/// `story:events-head-cursor` acceptance 1: `after=head` answers no event and the cursor after
/// the last one published, whatever `limit` it names, on the host's thread and beside it; a read
/// after that cursor answers what was published since. `limit=0` is still refused.
#[test]
fn after_head_answers_the_newest_cursor_and_no_event() {
    let mut host = scripted();
    let (status, whole) = routed(&mut host, "limit=10");
    assert_eq!(status, 200, "{whole}");
    let end = next(&whole);
    for query in ["after=head", "limit=5&after=head", "after=head&limit=1000"] {
        let (status, head) = routed(&mut host, query);
        assert_eq!(status, 200, "{query}: {head}");
        assert_eq!(events(&head), Vec::<Value>::new(), "{query}: no event");
        let (status, beside_head) = beside(&host, query);
        assert_eq!(status, 200, "{query}, beside: {beside_head}");
        assert_eq!(events(&beside_head), Vec::<Value>::new(), "{query}, beside");
        for cursor in [next(&head), next(&beside_head)] {
            let (status, idle) = routed(&mut host, &format!("after={cursor}"));
            assert_eq!(status, 200, "{idle}");
            assert_eq!(
                idle,
                json!({"events": [], "next": cursor}),
                "{query}: nothing after the head"
            );
        }
    }
    let (status, refused) = routed(&mut host, "after=head&limit=0");
    assert_eq!(status, 400, "a limit of 0 is refused: {refused}");
    let (_, head) = routed(&mut host, "after=head");
    let head = next(&head);

    submit(
        &mut host,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    for (status, page) in [
        routed(&mut host, &format!("after={head}")),
        beside(&host, &format!("after={head}")),
        routed(&mut host, &format!("after={end}")),
    ] {
        assert_eq!(status, 200, "{page}");
        let ids = events(&page)
            .iter()
            .map(|event| event["payload"]["task_id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, [json!("t3")], "only what was published since: {page}");
    }
}

/// Acceptance 4: without a query, or with only parameters it does not read, the answer is
/// `{"events": [...]}` as before, every event, no `next`.
#[test]
fn without_a_query_the_answer_is_as_before() {
    let mut host = scripted();
    let before = console::events(&mut host);
    assert_eq!(before.status, 200);
    let before = before.body.expect("a body");
    assert_eq!(
        before
            .as_object()
            .map(|body| body.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["events".to_owned()])
    );
    for query in ["", "since=0", "cursor=x&offset=2"] {
        let (status, body) = routed(&mut host, query);
        assert_eq!(status, 200, "{query}: {body}");
        assert_eq!(body, before, "{query}");
        let (status, body) = beside(&host, query);
        assert_eq!(status, 200, "{query}: {body}");
        assert_eq!(body, before, "{query}, beside");
    }
}

/// One `Connection: close` request: the status and the body as JSON.
fn send(address: SocketAddr, target: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body}"));
    (status, body)
}

/// The newest record of the request log.
fn newest(address: SocketAddr) -> Value {
    let (status, log) = send(address, "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    log["requests"][0].clone()
}

/// Acceptance 5: the request log records the query string of a console request: the cursor of
/// an answered read and a limit kept, which is what a poller needs, every other name and value
/// written `…`, at most 2,048 bytes as the path is.
#[test]
fn the_request_log_records_a_console_requests_query() {
    let mut host = scripted();
    let ir = host.ir().clone();
    let observed = surface(&ir).observing(16);
    let listener = Listener::bind_reading(0, observed, host.reader()).expect("the listener binds");
    let address = listener.local_addr();
    let (_, page) = routed(&mut host, "limit=1");
    let cursor = next(&page);

    let (status, body) = send(
        address,
        &format!("/api/events?after={cursor}&limit=2&since=secret-since&&flag"),
    );
    assert_eq!(status, 200, "{body}");
    let record = newest(address);
    assert_eq!(record["path"], "/api/events", "{record}");
    assert_eq!(
        record["query"],
        format!("after={cursor}&limit=2&…=…&…"),
        "{record}"
    );

    let (status, body) = send(address, "/api/events?after=secret-cursor&limit=1");
    assert_eq!(status, 400, "{body}");
    let record = newest(address);
    assert_eq!(
        record["query"], "after=…&limit=1",
        "a cursor that was refused is caller text: {record}"
    );
    assert!(!record.to_string().contains("secret"), "{record}");

    let (status, body) = send(address, "/api/events?limit=secret");
    assert_eq!(status, 400, "{body}");
    assert_eq!(newest(address)["query"], "limit=…");

    let (status, body) = send(address, "/api/events");
    assert_eq!(status, 200, "{body}");
    let record = newest(address);
    assert!(
        record.get("query").is_none(),
        "no query, none recorded: {record}"
    );

    let long = format!("/api/events?limit=1&{}", "p=1&".repeat(2_000));
    let (status, body) = send(address, &long);
    assert_eq!(status, 200, "{body}");
    let kept = newest(address)["query"]
        .as_str()
        .expect("a query")
        .to_owned();
    assert!(kept.len() <= PATH_MOST, "{} bytes", kept.len());
    assert!(kept.len() > PATH_MOST - 8, "{} bytes", kept.len());
    assert!(kept.starts_with("limit=1&…=…&"), "{kept}");
    assert!(kept.ends_with('…'), "the cut is marked: {kept}");
}
