//! Two slow reads run at once (`story:concurrent-reads`).
//!
//! A listener given the host's [`Reader`] answers each view read on a thread of its own, beside
//! the host's thread, so two reads that each take [`SLOW`] answer in about [`SLOW`] each, not in
//! the sum. The read is slow by its store, not by its size: a supplied store whose every scan,
//! by the host or through its reading handle, takes [`SLOW`]. Each pair is timed [`RUNS`] times:
//! the median of the slower read of each pair within [`BUDGET`], well under the sum, and every
//! one within [`CEILING`].
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, owning `Desk`, listed by the
//! view `study.desk.AllDesks`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Exchange, Listener, Surface};
use spindle_kernel::{
    Change, MemoryStore, Position, Reading, Removal, Scan, Store, StoreError, Stored,
    Write as Written,
};

/// How long every scan of the slow store takes.
const SLOW: Duration = Duration::from_millis(600);
/// How many pairs of reads are timed.
const RUNS: usize = 5;
/// The median the slower read of a pair may take: half a read over one, where two in turn take
/// two.
const BUDGET: Duration = Duration::from_millis(900);
/// How long any one read may take.
const CEILING: Duration = Duration::from_secs(10);
const COMPONENT: &str = "desk";
const VIEW: &str = "study.desk.AllDesks";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn environment() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"slow-reads\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"elsewhere\"]\n",
        "\"store.ephemeral\" = [\"elsewhere\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the environment parses")
}

/// A memory store whose every scan takes [`SLOW`], by its writer or through its reading handle.
#[derive(Default)]
struct Slow(MemoryStore);

impl Store for Slow {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.0.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.0.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Written]) -> Result<(), StoreError> {
        self.0.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        thread::sleep(SLOW);
        self.0.scan(entity)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        let inner = self.0.reading()?;
        Some(Arc::new(SlowReading(inner)))
    }
}

/// The reading handle of a [`Slow`] store.
struct SlowReading(Arc<dyn Reading>);

impl Reading for SlowReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        thread::sleep(SLOW);
        self.0.scan(entity)
    }

    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        thread::sleep(SLOW);
        self.0.at(entities)
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// `GET target`: the status and the body.
fn get(address: SocketAddr, target: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
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
    (status, body.to_owned())
}

#[test]
fn two_slow_reads_each_answer_in_about_their_own_time() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface =
        Surface::new(&ir, &[COMPONENT.to_owned()], None).expect("the desk surface is served");
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == VIEW)
        .expect("the surface serves the view")
        .path
        .clone();
    let mut host = Host::supplied(
        ir,
        &environment(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([(
            COMPONENT.to_owned(),
            Box::new(Slow::default()) as Box<dyn Store>,
        )]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the slow store: {error}"));
    host.submit(
        "study.desk.OpenDesk",
        object(json!({"desk_id": "desk-1", "holder": "a reader"})),
    )
    .expect("OpenDesk runs");
    let listener = Listener::bind_reading(0, surface, host.reader()).expect("the listener binds");
    let address = listener.local_addr();

    let pairs = thread::spawn(move || {
        (0..RUNS)
            .map(|_| {
                let pair = (0..2)
                    .map(|_| {
                        let view = view.clone();
                        thread::spawn(move || {
                            let started = Instant::now();
                            let (status, body) = get(address, &view);
                            (started.elapsed(), status, body)
                        })
                    })
                    .collect::<Vec<_>>();
                pair.into_iter()
                    .map(|read| read.join().expect("the read's thread ends"))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    });
    // The host's thread, as a served run's: every exchange the listener hands it is answered here.
    while !pairs.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let pairs = pairs.join().expect("the timing thread ends");

    let mut slower = Vec::new();
    for pair in &pairs {
        for (_, status, body) in pair {
            assert_eq!(*status, 200, "{body}");
            let rows: Value = serde_json::from_str(body).expect("the view answers JSON");
            assert_eq!(rows["rows"][0]["desk_id"], "desk-1", "{body}");
        }
        slower.push(
            pair.iter()
                .map(|(took, ..)| *took)
                .max()
                .expect("two reads"),
        );
    }
    let timed = slower.clone();
    slower.sort();
    let median = slower[slower.len() / 2];
    eprintln!("the slower of two reads of {SLOW:?} each: median {median:?} of {timed:?}");
    assert!(
        median < BUDGET,
        "the slower of two reads of {SLOW:?} each took {median:?} at the median, over {BUDGET:?}: \
         {timed:?}"
    );
    assert!(
        slower.iter().all(|took| *took < CEILING),
        "a read took over {CEILING:?}: {timed:?}"
    );
}
