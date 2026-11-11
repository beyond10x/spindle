//! A read beside the host's thread never sees a partly applied command
//! (`story:concurrent-reads`).
//!
//! A [`Reader`] taken from a host reads its views from another thread while the host runs
//! commands. Over drawn sequences of commands (set outcomes among them, which write many rows in
//! one commit) and resets that load a world, on the memory and the SQLite store, every list a
//! reader thread read while the sequence ran is the list the host answered after some prefix of
//! the sequence, and the prefixes the reads saw never go back.
//!
//! The fixture `tests/fixtures/rows` declares one component, `desk`, owning `Loan` (created,
//! moved, updated and removed one at a time and as a set) and `Member`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Reader, Seed, World};
use spindle_kernel::Row;

const LOAN: &str = "lending.loans.Loan";
/// The view that lists every loan, with its state, borrower and due time.
const VIEW: &str = "lending.loans.LoanCard";
/// How many sequences each store runs, and how many steps each takes.
const SEQUENCES: u64 = 4;
const STEPS: usize = 60;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rows")
}

pub(crate) fn environment(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"reads\"\n",
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
        format!("loan-{}", self.below(10))
    }

    fn borrower(&mut self) -> String {
        format!("reader-{}", self.below(3))
    }

    fn instant(&mut self) -> String {
        format!("2026-01-{:02}T00:00:00Z", 1 + self.below(9))
    }

    fn seed(&mut self) -> Seed {
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
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// Runs one drawn step on `host`; what it did, for a failure message. A command the model
/// refuses is a step like any other. Lends are drawn most, so the set outcomes find many rows.
fn step(draw: &mut Draw, host: &mut Host) -> String {
    let (command, input) = match draw.below(12) {
        0..=4 => (
            "lending.loans.Lend",
            json!({"loan_id": draw.loan(), "borrower": draw.borrower(), "due": draw.instant()}),
        ),
        5 => ("lending.loans.Return", json!({"loan_id": draw.loan()})),
        6 => (
            "lending.loans.MarkOverdue",
            json!({"as_of": draw.instant()}),
        ),
        7 | 8 => (
            "lending.loans.Renew",
            json!({"borrower": draw.borrower(), "until": draw.instant()}),
        ),
        9 => ("lending.loans.Purge", json!({"borrower": draw.borrower()})),
        10 => ("lending.loans.Forget", json!({"loan_id": draw.loan()})),
        _ => {
            let mut rows: Vec<Seed> = Vec::new();
            for _ in 0..draw.below(6) {
                let seed = draw.seed();
                if !rows.iter().any(|held| held.identity == seed.identity) {
                    rows.push(seed);
                }
            }
            let done = format!("reset to {} rows", rows.len());
            host.reset_to(World {
                rows,
                answers: None,
                mint: None,
            })
            .expect("the reset loads its world");
            return done;
        }
    };
    let done = format!("{command} {input}");
    let _ = host.submit(command, object(input));
    done
}

/// Reads `VIEW` through `reader` until `stop`, every list it read in order.
fn reading(reader: &Reader, stop: &AtomicBool) -> Vec<Vec<Map<String, Value>>> {
    let mut read = Vec::new();
    loop {
        let stopping = stop.load(Ordering::SeqCst);
        read.push(
            reader
                .list(VIEW)
                .unwrap_or_else(|error| panic!("{VIEW} is read beside the host: {error}")),
        );
        if stopping {
            return read;
        }
    }
}

#[test]
fn every_read_beside_the_host_is_the_state_after_a_prefix_of_the_commands() {
    let mut between = 0;
    for store in ["memory", "sqlite"] {
        between += prefixes_read(store, &|state: &Path| {
            Host::new(
                rows_fixture(),
                &environment(store),
                Arc::new(ManualClock::new()),
                state,
            )
            .unwrap_or_else(|error| panic!("the {store} host opens: {error}"))
        });
    }
    assert!(
        between > 0,
        "no read landed between the first and the last step: the reads did not run beside the \
         commands"
    );
}

/// The fixture's specification, compiled.
pub(crate) fn rows_fixture() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the rows fixture compiles")
        .ir
}

/// Runs [`SEQUENCES`] drawn sequences on hosts `open` opens on a state directory, a reader
/// thread reading beside each, and holds every read to the state after a prefix of its
/// sequence; answers how many reads landed between the first and the last step.
pub(crate) fn prefixes_read(store: &str, open: &dyn Fn(&Path) -> Host) -> usize {
    let mut between = 0;
    for seed in 0..SEQUENCES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
        let mut host = open(state.path());
        let reader = host.reader();
        let stop = Arc::new(AtomicBool::new(false));
        let reads = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || reading(&reader, &stop))
        };

        let mut draw = Draw::new(seed);
        let mut done = Vec::new();
        let mut prefixes = vec![host.list(VIEW).expect("the view lists")];
        for _ in 0..STEPS {
            done.push(step(&mut draw, &mut host));
            prefixes.push(host.list(VIEW).expect("the view lists"));
        }
        stop.store(true, Ordering::SeqCst);
        let reads = reads.join().expect("the reader thread ends");

        // Each read is the state after some prefix, at or after the prefix the read before
        // it saw: the earliest such prefix, so a later read is never held to a later one
        // than it must be.
        let mut at = 0;
        for (index, read) in reads.iter().enumerate() {
            let Some(found) = prefixes[at..].iter().position(|prefix| prefix == read) else {
                panic!(
                    "{store}, seed {seed}: read {index} of {} is the state after no prefix \
                         from {at} on: {read:#?}\nafter {done:#?}",
                    reads.len()
                );
            };
            at += found;
            if at > 0 && at < STEPS {
                between += 1;
            }
        }
        assert_eq!(
            reads.last(),
            prefixes.last(),
            "{store}, seed {seed}: the last read, after the sequence, is its end"
        );
    }
    between
}
