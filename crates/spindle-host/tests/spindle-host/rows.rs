//! The rows each entity holds, as the host keeps them (`story:metrics-read-without-scan`): taken
//! from the store once when it opens, then moved by each write that lands. After every step of a
//! drawn sequence of commands, establishments, resets and restarts, on the memory, SQLite and a
//! supplied store, each entity's kept count is the number of rows a scan of its store answers;
//! and reading them scans no store.
//!
//! The fixture `tests/fixtures/rows` declares one component, `desk`, owning `Loan` (created,
//! moved, updated and removed one at a time and as a set) and `Member` (created, moved and
//! removed).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Seed, Stores, World};
use spindle_kernel::{Change, MemoryStore, Removal, Row, Store, StoreError, Stored};

const LOAN: &str = "lending.loans.Loan";
const MEMBER: &str = "lending.loans.Member";
/// Each entity with the view that lists every row it holds.
const LISTED: [(&str, &str); 2] = [
    (LOAN, "lending.loans.LoanCard"),
    (MEMBER, "lending.loans.MemberCard"),
];
/// How many sequences each store runs, and how many steps each takes.
const SEQUENCES: u64 = 6;
const STEPS: usize = 70;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rows")
}

fn environment(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"rows\"\n",
            "os = \"linux\"\n",
            "arch = \"x86_64\"\n",
            "\n",
            "[provides]\n",
            "\"store.durable\" = [\"{store}\"]\n",
            "\"store.ephemeral\" = [\"{store}\"]\n",
            "\"identity.mint\" = [\"uuid-v4\"]\n",
            "clock = [\"system\"]\n",
            "channel = [\"in-process\"]\n",
            "projection = [\"in-process\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

/// A memory store the test keeps a handle to while the host writes through it, counting every
/// scan the host asks of it.
#[derive(Clone, Default)]
struct Shared {
    store: Arc<Mutex<MemoryStore>>,
    scans: Arc<AtomicUsize>,
}

impl Shared {
    /// How many rows of `entity` the store holds, read without counting a scan.
    fn held(&self, entity: &str) -> usize {
        self.store
            .lock()
            .expect("the store")
            .scan(entity)
            .expect("the store reads")
            .rows
            .len()
    }

    fn scans(&self) -> usize {
        self.scans.load(Ordering::SeqCst)
    }
}

impl Store for Shared {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.lock().expect("the store").remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.scans.fetch_add(1, Ordering::SeqCst);
        self.store.lock().expect("the store").scan(entity)
    }
}

/// Where a host of one sequence keeps its state, so a restart opens it again.
enum Kept {
    Memory,
    Sqlite(tempfile::TempDir),
    Supplied(Shared),
}

impl Kept {
    fn name(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Sqlite(_) => "sqlite",
            Self::Supplied(_) => "supplied",
        }
    }

    fn open(&self) -> Host {
        let ir = spindle_spec::load(&fixture())
            .expect("the rows fixture compiles")
            .ir;
        let clock = Arc::new(ManualClock::new());
        match self {
            Self::Memory => {
                let state =
                    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
                Host::new(ir, &environment("memory"), clock, state.path())
            }
            Self::Sqlite(state) => Host::new(ir, &environment("sqlite"), clock, state.path()),
            Self::Supplied(shared) => Host::supplied(
                ir,
                &environment("elsewhere"),
                clock,
                None,
                Stores::from([(
                    "desk".to_owned(),
                    Box::new(shared.clone()) as Box<dyn Store>,
                )]),
            ),
        }
        .unwrap_or_else(|error| panic!("the {} host opens: {error}", self.name()))
    }
}

/// A small deterministic generator (xorshift64*), so a failing sequence is replayed by its seed.
struct Draw(u64);

impl Draw {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % bound
    }

    fn loan(&mut self) -> String {
        format!("loan-{}", self.below(8))
    }

    fn member(&mut self) -> String {
        format!("member-{}", self.below(5))
    }

    fn borrower(&mut self) -> String {
        format!("reader-{}", self.below(3))
    }

    fn instant(&mut self) -> String {
        format!("2026-01-{:02}T00:00:00Z", 1 + self.below(9))
    }

    fn seed(&mut self) -> Seed {
        if self.below(2) == 0 {
            Seed {
                entity: LOAN.to_owned(),
                identity: json!(self.loan()),
                row: Row {
                    state: "Out".to_owned(),
                    fields: [
                        ("borrower".to_owned(), json!(self.borrower())),
                        ("due".to_owned(), json!(self.instant())),
                    ]
                    .into(),
                },
            }
        } else {
            Seed {
                entity: MEMBER.to_owned(),
                identity: json!(self.member()),
                row: Row {
                    state: "Active".to_owned(),
                    fields: [("name".to_owned(), json!("A. Reader"))].into(),
                },
            }
        }
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// Runs one drawn step on `host`, which a restart replaces; what it did, for a failure message.
/// A command the model refuses, or a write the store refuses, is a step like any other.
fn step(draw: &mut Draw, kept: &Kept, host: &mut Host) -> String {
    let (command, input) = match draw.below(15) {
        0..=2 => (
            "lending.loans.Lend",
            json!({"loan_id": draw.loan(), "borrower": draw.borrower(), "due": draw.instant()}),
        ),
        3 => ("lending.loans.Return", json!({"loan_id": draw.loan()})),
        4 => (
            "lending.loans.MarkOverdue",
            json!({"as_of": draw.instant()}),
        ),
        5 => (
            "lending.loans.Renew",
            json!({"borrower": draw.borrower(), "until": draw.instant()}),
        ),
        6 => ("lending.loans.Purge", json!({"borrower": draw.borrower()})),
        7 => ("lending.loans.Forget", json!({"loan_id": draw.loan()})),
        8 => (
            "lending.loans.Join",
            json!({"member_id": draw.member(), "name": "A. Reader"}),
        ),
        9 => ("lending.loans.Leave", json!({"member_id": draw.member()})),
        10 => ("lending.loans.Lapse", json!({"member_id": draw.member()})),
        11 => {
            let seed = draw.seed();
            let done = format!("establish {} {}", seed.entity, seed.identity);
            let _ = host.establish(&seed.entity, &seed.identity, seed.row);
            return done;
        }
        12 => {
            host.reset().expect("the reset");
            return "reset".to_owned();
        }
        13 => {
            let rows = (0..draw.below(4)).map(|_| draw.seed()).collect::<Vec<_>>();
            let done = format!("reset to {} rows", rows.len());
            let _ = host.reset_to(World {
                rows,
                answers: None,
                mint: None,
            });
            return done;
        }
        _ => {
            *host = kept.open();
            return "restart".to_owned();
        }
    };
    let done = format!("{command} {input}");
    let _ = host.submit(command, object(input));
    done
}

/// What a scan of each entity's store answers, read through the host's list views (each lists
/// every row its entity holds), and on a supplied store from the store itself too.
fn scanned(kept: &Kept, host: &mut Host) -> Vec<(String, usize)> {
    LISTED
        .iter()
        .map(|(entity, view)| {
            let listed = host
                .list(view)
                .unwrap_or_else(|error| panic!("{view} lists: {error}"))
                .len();
            if let Kept::Supplied(shared) = kept {
                assert_eq!(
                    shared.held(entity),
                    listed,
                    "{entity} on the supplied store"
                );
            }
            ((*entity).to_owned(), listed)
        })
        .collect()
}

#[test]
fn every_kept_count_is_the_rows_a_scan_answers_after_every_step() {
    // What the drawn steps did to the counts, on any store: each entity held more than one row,
    // and a command made one fall, so the sequences move every count both ways.
    let (mut most, mut fell) = ([0; LISTED.len()], [false; LISTED.len()]);
    for seed in 0..SEQUENCES {
        for kept in [
            Kept::Memory,
            Kept::Sqlite(
                tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory"),
            ),
            Kept::Supplied(Shared::default()),
        ] {
            let mut draw = Draw::new(seed);
            let mut host = kept.open();
            let mut done = Vec::new();
            let mut before = vec![0; LISTED.len()];
            for _ in 0..STEPS {
                done.push(step(&mut draw, &kept, &mut host));
                let rows = host
                    .rows()
                    .unwrap_or_else(|error| panic!("the rows are read: {error}"));
                assert_eq!(
                    rows,
                    scanned(&kept, &mut host),
                    "seed {seed} on {}, after {done:#?}",
                    kept.name()
                );
                let commanded = done.last().is_some_and(|step| step.starts_with("lending."));
                for (index, (_, held)) in rows.iter().enumerate() {
                    most[index] = most[index].max(*held);
                    fell[index] |= commanded && *held < before[index];
                    before[index] = *held;
                }
            }
        }
    }
    assert!(
        most.iter().all(|most| *most > 1),
        "most rows held: {most:?}"
    );
    assert_eq!(
        fell,
        [true; LISTED.len()],
        "a command removed a row of each"
    );
}

#[test]
fn reading_the_rows_scans_no_store() {
    let shared = Shared::default();
    let kept = Kept::Supplied(shared.clone());
    let mut host = kept.open();
    for loan in 0..4 {
        host.submit(
            "lending.loans.Lend",
            object(json!({
                "loan_id": format!("loan-{loan}"),
                "borrower": "reader-0",
                "due": "2026-01-01T00:00:00Z",
            })),
        )
        .expect("Lend runs");
    }
    host.submit(
        "lending.loans.Join",
        object(json!({"member_id": "member-0", "name": "A. Reader"})),
    )
    .expect("Join runs");
    host.submit("lending.loans.Forget", object(json!({"loan_id": "loan-3"})))
        .expect("Forget runs");

    let before = shared.scans();
    let rows = host.rows().expect("the rows are read");
    assert_eq!(
        shared.scans() - before,
        0,
        "reading the rows scanned the store"
    );
    assert_eq!(rows, vec![(LOAN.to_owned(), 3), (MEMBER.to_owned(), 1)]);
}
