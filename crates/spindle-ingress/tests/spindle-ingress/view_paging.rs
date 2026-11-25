//! Views that declare paging, served (`story:view-paging`): the page and the size read from the
//! query under the names `paging:` declares, the rows cut as ESS 0.99.1's interpreter cuts them,
//! and `total` beside the rows only where the view declares `total: true`.
//!
//! The fixture `tests/fixtures/served-operations` holds three paged views over `notes.pad.Note`:
//! `NotePages` (`order_by: [note_id asc]`, `first_page` 0, `total: true`), `NotePagesByText`
//! (`p` and `n`, `order_by: [text asc]`, `first_page: 1`, no `total`) and `FilteredNotePages`
//! (`filter: text != "c"`, `first_page` 0, `total: true`). Every test writes the same five notes,
//! [`NOTES`], on the host and on ESS's interpreter.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ess_compiler::EssIr;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{HTTP_LOOPBACK, Listener, Request, Surface};

const COMPONENT: &str = "pad";
const WRITE: &str = "notes.pad.WriteNote";
const PAGES: &str = "notes.pad.NotePages";
const BY_TEXT: &str = "notes.pad.NotePagesByText";
const FILTERED: &str = "notes.pad.FilteredNotePages";

/// The notes every test writes, `(note_id, text)`.
const NOTES: [(&str, &str); 5] = [
    ("n1", "b"),
    ("n2", "a"),
    ("n3", "b"),
    ("n4", "c"),
    ("n5", "a"),
];

/// Each paged view: its name, the route it is read at, the query keys of its page and size, and
/// its first page.
const PAGED: [(&str, &str, &str, &str, i64); 3] = [
    (PAGES, "/pad/views/NotePages", "page", "size", 0),
    (BY_TEXT, "/pad/views/NotePagesByText", "p", "n", 1),
    (FILTERED, "/pad/views/FilteredNotePages", "page", "size", 0),
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("served-operations"))
        .expect("the served-operations fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"paging\"\n",
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

/// The host serving the fixture and ESS's interpreter, each with [`NOTES`] written.
struct Pad {
    surface: Surface,
    host: Host,
    interpreter: Interpreted,
    _state: tempfile::TempDir,
}

impl Pad {
    fn open() -> Self {
        let ir = ir();
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
            .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
            .unwrap_or_else(|refused| panic!("the pad surface is served: {refused}"));
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = Host::new(
            ir.clone(),
            &in_memory(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("the host admits the pad: {error}"));
        let interpreter = Interpreted::for_model(ir.clone());
        let correlation = CorrelationId::new("view-paging").expect("a correlation");
        for (id, text) in NOTES {
            let input = json!({"note_id": id, "text": text});
            let Value::Object(fields) = input.clone() else {
                unreachable!("an object")
            };
            host.submit(WRITE, fields)
                .unwrap_or_else(|error| panic!("{id} is written on the host: {error}"));
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: WRITE.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input: serde_json::from_value(input).expect("the input as ESS values"),
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{id} is written on the interpreter: {error:?}"));
        }
        Self {
            surface,
            host,
            interpreter,
            _state: state,
        }
    }

    /// `GET <path>?<query>` through the router, on the host's thread: the status and the body.
    fn get(&mut self, path: &str, query: &str) -> (u16, Value) {
        let routed = spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: "GET".to_owned(),
                path: path.to_owned(),
                query: query.to_owned(),
                content_type: None,
                body: Vec::new(),
            },
        );
        let body = serde_json::from_str(&routed.response.body)
            .unwrap_or_else(|error| panic!("{error}: {}", routed.response.body));
        (routed.response.status, body)
    }

    /// What ESS's interpreter answers for `view` read with `params`, as the body ESS publishes
    /// for it: `{"rows": [...]}`, with `total` beside the rows where it answers one.
    fn interpreted(&self, view: &str, params: &[(&str, i64)]) -> Value {
        let params: BTreeMap<String, Node> = params
            .iter()
            .map(|(name, value)| {
                (
                    (*name).to_owned(),
                    serde_json::from_value(json!(value)).expect("an integer as an ESS value"),
                )
            })
            .collect();
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("view-paging").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("the interpreter reads {view}: {error:?}"));
        let mut body = Map::new();
        body.insert(
            "rows".to_owned(),
            serde_json::to_value(answer.rows).expect("rows as JSON"),
        );
        if let Some(total) = answer.total {
            body.insert("total".to_owned(), json!(total));
        }
        Value::Object(body)
    }
}

/// One HTTP/1.1 `GET` with `Connection: close`: the status and the JSON body.
fn request(address: SocketAddr, target: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
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
    let body = serde_json::from_str(body).unwrap_or_else(|_| panic!("not JSON: {body}"));
    (status, body)
}

/// The note identities of `body`'s rows, in order.
fn ids(body: &Value) -> Vec<&str> {
    body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer holds rows: {body}"))
        .iter()
        .map(|row| row["note_id"].as_str().expect("a note id"))
        .collect()
}

#[test]
fn a_paged_view_is_served() {
    let mut pad = Pad::open();
    let (status, body) = pad.get("/api/served", "");
    assert_eq!(status, 200, "{body}");
    let operations = body["operations"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists operations: {body}"));
    for (view, path, ..) in PAGED {
        let row = operations
            .iter()
            .find(|row| row["name"] == view)
            .unwrap_or_else(|| panic!("{view} is routed: {body:#}"));
        assert_eq!(row["served"], true, "{view} is served: {row}");
        assert_eq!(row["path"], path, "{row}");
        assert!(
            row.get("reason").is_none(),
            "{view} carries no reason: {row}"
        );
    }
    assert!(
        !pad.surface
            .unserved()
            .iter()
            .filter_map(|route| route.not_served())
            .any(|line| line.contains("paging")),
        "no route is refused for paging"
    );
}

#[test]
fn pages_match_the_ess_interpreter() {
    let mut pad = Pad::open();
    let listener = Listener::bind_reading(0, pad.surface.clone(), pad.host.reader())
        .expect("the reading listener binds");
    let address = listener.local_addr();
    let mut compared = 0;
    for (view, path, page, size, first) in PAGED {
        let admitted = ids(&pad.interpreted(view, &[])).len();
        assert!(admitted > 0, "{view} admits rows");
        for n in 1..=3_i64 {
            let size_of = usize::try_from(n).expect("a size");
            let pages = i64::try_from(admitted.div_ceil(size_of)).expect("a count");
            for p in first..=first + pages {
                let query = format!("{page}={p}&{size}={n}");
                let expected = pad.interpreted(view, &[(page, p), (size, n)]);
                let (status, body) = pad.get(path, &query);
                assert_eq!(status, 200, "{view} {query}: {body}");
                assert_eq!(body, expected, "{view} {query} on the host's thread");
                let (status, read) = request(address, &format!("{path}?{query}"));
                assert_eq!(status, 200, "{view} {query}: {read}");
                assert_eq!(read, expected, "{view} {query} through the reader");
                if p == first + pages {
                    assert_eq!(ids(&body), Vec::<&str>::new(), "one past the last page");
                }
                compared += 1;
            }
        }
    }
    assert_eq!(
        compared,
        6 + 4 + 3 + 6 + 4 + 3 + 5 + 3 + 3,
        "every page of every view was compared"
    );
}

#[test]
fn a_read_without_page_and_size_answers_every_row() {
    let mut pad = Pad::open();
    let (status, body) = pad.get("/pad/views/NotePages", "");
    assert_eq!(status, 200, "{body}");
    assert_eq!(ids(&body), ["n1", "n2", "n3", "n4", "n5"], "{body}");
    assert_eq!(body["total"], 5, "{body}");
    assert_eq!(body, pad.interpreted(PAGES, &[]));
}

#[test]
fn an_undeclared_total_is_not_answered() {
    let mut pad = Pad::open();
    let (status, body) = pad.get("/pad/views/NotePagesByText", "p=1&n=2");
    assert_eq!(status, 200, "{body}");
    assert_eq!(ids(&body), ["n2", "n5"], "{body}");
    assert!(body.get("total").is_none(), "no total: {body}");
    assert_eq!(body, pad.interpreted(BY_TEXT, &[("p", 1), ("n", 2)]));
}

#[test]
fn a_filtered_view_pages_the_admitted_rows() {
    let mut pad = Pad::open();
    let (status, body) = pad.get("/pad/views/FilteredNotePages", "page=1&size=2");
    assert_eq!(status, 200, "{body}");
    assert_eq!(ids(&body), ["n3", "n5"], "{body}");
    assert_eq!(body["total"], 4, "{body}");
    assert_eq!(body, pad.interpreted(FILTERED, &[("page", 1), ("size", 2)]));
}

#[test]
fn a_bad_page_is_answered_400() {
    let mut pad = Pad::open();
    let max = i64::MAX.to_string();
    for (path, query) in [
        ("/pad/views/NotePages", "page=1".to_owned()),
        ("/pad/views/NotePages", "size=2".to_owned()),
        ("/pad/views/NotePages", "page=-1&size=2".to_owned()),
        ("/pad/views/NotePages", "page=1&size=-2".to_owned()),
        ("/pad/views/NotePages", format!("page={max}&size=3")),
        ("/pad/views/NotePagesByText", "p=0&n=2".to_owned()),
    ] {
        let (status, body) = pad.get(path, &query);
        assert_eq!(status, 400, "{path}?{query}: {body}");
        assert!(
            body["refused"].is_string(),
            "{path}?{query} names its refusal: {body}"
        );
    }
}

// Adversary cases (story:view-paging): edges the acceptance tests do not reach, each compared with
// ESS 0.99.1's interpreter on the host's thread and through the reader.

/// A size of 0, a page far past the last, a size larger than the rows and the largest size, each
/// answered 200 with what ESS's interpreter answers.
#[test]
fn adversary_edge_pages_match_the_interpreter() {
    let mut pad = Pad::open();
    let listener = Listener::bind_reading(0, pad.surface.clone(), pad.host.reader())
        .expect("the reading listener binds");
    let address = listener.local_addr();
    let cases: [(i64, i64); 6] = [
        (0, 0),
        (3, 0),
        (1_000_000, 3),
        (0, 1000),
        (0, i64::MAX),
        (i64::MAX, 0),
    ];
    for (p, n) in cases {
        let query = format!("page={p}&size={n}");
        let expected = pad.interpreted(PAGES, &[("page", p), ("size", n)]);
        let (status, body) = pad.get("/pad/views/NotePages", &query);
        assert_eq!(status, 200, "{query}: {body}");
        assert_eq!(body, expected, "{query} on the host's thread");
        let (status, read) = request(address, &format!("/pad/views/NotePages?{query}"));
        assert_eq!(status, 200, "{query}: {read}");
        assert_eq!(read, expected, "{query} through the reader");
    }
}

/// A page or size that is not one whole number on the query is refused 400, never 200 or 500.
#[test]
fn adversary_malformed_page_values_are_answered_400() {
    let mut pad = Pad::open();
    for query in [
        "page=1.0&size=2",
        "page=1&size=2.5",
        "page=abc&size=2",
        "page=1&page=2&size=2",
        "page=9223372036854775808&size=1",
        "page=&size=2",
        "page=1&size=",
        "page=1e0&size=2",
    ] {
        let (status, body) = pad.get("/pad/views/NotePages", query);
        assert_eq!(status, 400, "{query}: {body}");
        assert!(body["refused"].is_string(), "{query}: {body}");
    }
}

/// A model of its own: books with `Integer` identities, read by paged views whose page is a newtype
/// of `Integer` under a wire name, whose size is `Optional<Integer>`, over a descending order with
/// ties, and beside a filter parameter. (ESS refuses `order_by:` on an aggregate view, and
/// `paging:` requires it, so no paged aggregation compiles.)
const SHELF_SYSTEM: &str = "format: ess/34
system: shelf
version: v1
summary: Books paged.
domains:
  - shelf.book
components:
  - component: shelf
    summary: Holds books.
    owns:
      domains:
        - shelf.book
    accepts:
      commands:
        - shelf.book.AddBook
    publishes:
      events:
        - shelf.book.BookAdded
    reached_by: network
";

const SHELF_DOMAIN: &str = "domain: shelf.book
summary: Books, paged.
types:
  - name: shelf.book.PageNo
    kind: newtype
    of: Integer
entities:
  - name: shelf.book.Book
    identity:
      name: book_id
      type: Integer
    fields:
      - name: title
        type: String
      - name: stars
        type: Integer
    lifecycle:
      initial: Kept
      states: [Kept]
      terminal: [Kept]
commands:
  - name: shelf.book.AddBook
    input:
      - name: book_id
        type: Integer
      - name: title
        type: String
      - name: stars
        type: Integer
    outcomes:
      - name: added
        creates: shelf.book.Book
        instance: book_id
        sets:
          title: input.title
          stars: input.stars
        emits: [shelf.book.BookAdded]
        payload:
          shelf.book.BookAdded: {book_id: input.book_id}
        summary: The book is kept.
events:
  - name: shelf.book.BookAdded
    fields:
      - name: book_id
        type: Integer
views:
  - name: shelf.book.ByStarsDesc
    source: shelf.book.Book
    consistency: read_your_writes
    params:
      - {name: page, type: shelf.book.PageNo, wire: pageNumber}
      - {name: size, type: Optional<Integer>, wire: pageSize}
    order_by: [stars desc]
    paging: {page: page, size: size, first_page: 1, total: true}
    fields:
      - {name: book_id, type: Integer}
      - {name: title, type: String}
      - {name: stars, type: Integer}
  - name: shelf.book.Starred
    source: shelf.book.Book
    consistency: read_your_writes
    params:
      - {name: min, type: Integer}
      - {name: page, type: Integer}
      - {name: size, type: Integer}
    filter: stars >= param.min
    order_by: [title asc]
    paging: {page: page, size: size, total: true}
    fields:
      - {name: book_id, type: Integer}
      - {name: title, type: String}
";

/// The books written, `(book_id, title, stars)`: four tied at 3 stars, whose identities order
/// `1, 2, 10, 20` as numbers and `1, 10, 2, 20` as text.
const BOOKS: [(i64, &str, i64); 6] = [
    (1, "x", 3),
    (2, "y", 3),
    (10, "x", 3),
    (9, "z", 5),
    (100, "y", 1),
    (20, "x", 3),
];

/// The shelf model served by a host and read by ESS's interpreter, each with [`BOOKS`] written.
struct Shelf {
    surface: Surface,
    host: Host,
    interpreter: Interpreted,
    _spec: tempfile::TempDir,
    _state: tempfile::TempDir,
}

impl Shelf {
    fn open() -> Self {
        let spec = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a spec directory");
        std::fs::create_dir(spec.path().join("domains")).expect("the domains directory");
        std::fs::write(spec.path().join("system.yaml"), SHELF_SYSTEM).expect("the system");
        std::fs::write(spec.path().join("domains/book.yaml"), SHELF_DOMAIN).expect("the domain");
        let ir = spindle_spec::load_model(spec.path())
            .unwrap_or_else(|error| panic!("the shelf compiles: {error:?}"))
            .ir;
        let surface = Surface::new(&ir, &["shelf".to_owned()], None)
            .unwrap_or_else(|refused| panic!("the shelf surface is served: {refused}"));
        assert!(
            surface.unserved().is_empty(),
            "every shelf route is served: {:?}",
            surface
                .unserved()
                .iter()
                .filter_map(|route| route.not_served())
                .collect::<Vec<_>>()
        );
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = Host::new(
            ir.clone(),
            &in_memory(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("the host admits the shelf: {error}"));
        let interpreter = Interpreted::for_model(ir);
        let correlation = CorrelationId::new("view-paging").expect("a correlation");
        for (id, title, stars) in BOOKS {
            let input = json!({"book_id": id, "title": title, "stars": stars});
            let Value::Object(fields) = input.clone() else {
                unreachable!("an object")
            };
            host.submit("shelf.book.AddBook", fields)
                .unwrap_or_else(|error| panic!("{id} is written on the host: {error}"));
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: "shelf.book.AddBook".parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input: serde_json::from_value(input).expect("the input as ESS values"),
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{id} is written on the interpreter: {error:?}"));
        }
        Self {
            surface,
            host,
            interpreter,
            _spec: spec,
            _state: state,
        }
    }

    fn get(&mut self, path: &str, query: &str) -> (u16, Value) {
        let routed = spindle_ingress::route(
            &self.surface,
            &mut self.host,
            Request {
                method: "GET".to_owned(),
                path: path.to_owned(),
                query: query.to_owned(),
                content_type: None,
                body: Vec::new(),
            },
        );
        let body = serde_json::from_str(&routed.response.body)
            .unwrap_or_else(|error| panic!("{error}: {}", routed.response.body));
        (routed.response.status, body)
    }

    fn interpreted(&self, view: &str, params: &[(&str, i64)]) -> Value {
        let params: BTreeMap<String, Node> = params
            .iter()
            .map(|(name, value)| {
                (
                    (*name).to_owned(),
                    serde_json::from_value(json!(value)).expect("an integer as an ESS value"),
                )
            })
            .collect();
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("view-paging").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("the interpreter reads {view}: {error:?}"));
        let mut body = Map::new();
        body.insert(
            "rows".to_owned(),
            integral(serde_json::to_value(answer.rows).expect("rows as JSON")),
        );
        if let Some(total) = answer.total {
            body.insert("total".to_owned(), json!(total));
        }
        Value::Object(body)
    }

    /// Every page of `view` at `path` for sizes 1 to 4, from `first` to one past the last, on the
    /// host's thread and through the reader, against the interpreter. `fixed` is sent with every
    /// read, under the same name on the query and to the interpreter.
    fn compare(
        &mut self,
        view: &str,
        path: &str,
        wire: (&str, &str),
        names: (&str, &str),
        first: i64,
        fixed: &[(&str, i64)],
    ) {
        let short = view.rsplit('.').next().expect("a short name");
        let routed = self
            .surface
            .routes()
            .iter()
            .map(|route| route.path.clone())
            .find(|routed| routed.ends_with(&format!("/{short}")))
            .unwrap_or_else(|| {
                panic!(
                    "{view} is routed: {:?}",
                    self.surface
                        .routes()
                        .iter()
                        .map(|route| &route.path)
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(routed, path, "{view} is routed at {routed}");
        let listener = Listener::bind_reading(0, self.surface.clone(), self.host.reader())
            .expect("the reading listener binds");
        let address = listener.local_addr();
        let all = self.interpreted(view, fixed);
        let admitted = all["rows"].as_array().expect("rows").len();
        assert!(admitted > 1, "{view} admits rows: {all}");
        let prefix: String = fixed
            .iter()
            .map(|(name, value)| format!("{name}={value}&"))
            .collect();
        let (status, body) = self.get(path, prefix.trim_end_matches('&'));
        assert_eq!(status, 200, "{view} unpaged: {body}");
        assert_eq!(body, all, "{view} unpaged on the host's thread");
        for n in 1..=4_i64 {
            let pages = i64::try_from(admitted.div_ceil(usize::try_from(n).expect("a size")))
                .expect("a count");
            for p in first..=first + pages {
                let query = format!("{prefix}{}={p}&{}={n}", wire.0, wire.1);
                let mut params = fixed.to_vec();
                params.push((names.0, p));
                params.push((names.1, n));
                let expected = self.interpreted(view, &params);
                let (status, body) = self.get(path, &query);
                assert_eq!(status, 200, "{view} {query}: {body}");
                assert_eq!(body, expected, "{view} {query} on the host's thread");
                let (status, read) = request(address, &format!("{path}?{query}"));
                assert_eq!(status, 200, "{view} {query}: {read}");
                assert_eq!(read, expected, "{view} {query} through the reader");
            }
        }
    }
}

/// `value` with each integral number written as an integer, as an ESS `Integer` is answered on the
/// wire; ESS's interpreter's rows serialize an `Integer` as a float.
fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            #[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
            Some(float) if float.fract() == 0.0 && number.as_i64().is_none() => {
                json!(float as i64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name, integral(value)))
                .collect(),
        ),
        other => other,
    }
}

/// Tied rows over `Integer` identities, a page that is a newtype under a wire name and an
/// `Optional<Integer>` size: every page as ESS's interpreter answers it.
#[test]
fn adversary_integer_identity_ties_page_as_the_interpreter_pages_them() {
    let mut shelf = Shelf::open();
    shelf.compare(
        "shelf.book.ByStarsDesc",
        "/book/views/ByStarsDesc",
        ("pageNumber", "pageSize"),
        ("page", "size"),
        1,
        &[],
    );
}

/// A paged view beside a required filter parameter, ordered by a text field with ties: every page
/// as ESS's interpreter answers it.
#[test]
fn adversary_a_filter_parameter_beside_paging_pages_as_the_interpreter_pages_it() {
    let mut shelf = Shelf::open();
    shelf.compare(
        "shelf.book.Starred",
        "/book/views/Starred",
        ("page", "size"),
        ("page", "size"),
        0,
        &[("min", 3)],
    );
}
