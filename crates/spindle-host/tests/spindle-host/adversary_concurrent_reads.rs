//! Adversarial cases for `story:concurrent-reads`: what a [`Reader`] reads beside the host's
//! thread, and what reading costs the writer.
//!
//! The fixture `tests/fixtures/rows` declares one component, `desk`, owning `Loan` and `Member`.
//!
//! - The rows a reader counts (`Reader::rows`, what `GET /__spindle/metrics` reports as
//!   `spindle_entity_rows`) are read beside the host's thread too, so they must be a state after
//!   some command or reset, never part of one: not a set removal half counted, not a reset half
//!   loaded.
//! - The writer must not pay for a read: a read of one small entity, or of the events published,
//!   must not make every write copy the whole store or the whole event history.
//! - A burst of reads on a SQLite store must not leave a connection per read open for good.
//! - Many resets must not leave the files of earlier stores open.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Reader, Seed, World};
use spindle_kernel::Row;

const LOAN: &str = "lending.loans.Loan";
const LOANS: &str = "lending.loans.LoanCard";
const MEMBERS: &str = "lending.loans.MemberCard";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rows")
}

fn environment(store: &str) -> Environment {
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

fn host(store: &str, state: &Path) -> Host {
    let ir = spindle_spec::load(&fixture())
        .expect("the rows fixture compiles")
        .ir;
    Host::new(ir, &environment(store), Arc::new(ManualClock::new()), state)
        .unwrap_or_else(|error| panic!("the {store} host opens: {error}"))
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn submit(host: &mut Host, command: &str, input: Value) {
    host.submit(command, object(input))
        .unwrap_or_else(|error| panic!("{command} runs: {error}"));
}

/// A world of `count` loans `prefix-<n>`, each out to `borrower` and due on 2026-01-01.
fn world(prefix: &str, count: usize, borrower: &str) -> World {
    World {
        rows: (0..count)
            .map(|index| Seed {
                entity: LOAN.to_owned(),
                identity: json!(format!("{prefix}-{index}")),
                row: Row {
                    state: "Out".to_owned(),
                    fields: [
                        ("borrower".to_owned(), json!(borrower)),
                        ("due".to_owned(), json!("2026-01-01T00:00:00Z")),
                    ]
                    .into(),
                },
            })
            .collect(),
        answers: None,
        mint: None,
    }
}

/// How many loans `reader` counts now.
fn loans(reader: &Reader) -> usize {
    reader
        .rows()
        .expect("the reader counts the rows")
        .into_iter()
        .find(|(entity, _)| entity == LOAN || entity.ends_with(".Loan"))
        .map(|(_, count)| count)
        .expect("the reader counts loans")
}

const IDLE: u8 = 0;
const WATCHED: u8 = 1;

/// Counts loans through `reader` until `stop`, keeping each count read wholly while `phase` was
/// [`WATCHED`]: read after the phase was seen watched and before it was seen watched again.
fn sample(reader: &Reader, phase: &AtomicU8, stop: &AtomicBool) -> Vec<usize> {
    let mut kept = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        let before = phase.load(Ordering::SeqCst);
        let count = loans(reader);
        let after = phase.load(Ordering::SeqCst);
        if before == WATCHED && after == WATCHED {
            kept.push(count);
        }
    }
    kept
}

/// A reset reaches its readers whole ([`Reader`]: "a reset hands its readers the emptied stores
/// only once every store is emptied and its world loaded"): a count of the rows read while it
/// runs is the count before it or the count of the world it loads. `GET /__spindle/metrics`
/// reports this count as `spindle_entity_rows` beside the host's thread.
#[test]
fn adversary_a_reader_counts_the_rows_before_a_reset_or_after_it_never_between() {
    const BEFORE: usize = 5;
    const AFTER: usize = 300;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("memory", state.path());
    let reader = host.reader();
    let phase = Arc::new(AtomicU8::new(IDLE));
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (phase, stop) = (Arc::clone(&phase), Arc::clone(&stop));
        std::thread::spawn(move || sample(&reader, &phase, &stop))
    };
    for _ in 0..20 {
        host.reset_to(world("before", BEFORE, "reader-0"))
            .expect("the small world loads");
        phase.store(WATCHED, Ordering::SeqCst);
        host.reset_to(world("after", AFTER, "reader-0"))
            .expect("the large world loads");
        phase.store(IDLE, Ordering::SeqCst);
    }
    stop.store(true, Ordering::SeqCst);
    let kept = sampler.join().expect("the sampler ends");
    assert!(
        !kept.is_empty(),
        "no count was read while a reset ran: the case observed nothing"
    );
    let between = kept
        .iter()
        .filter(|count| **count != BEFORE && **count != AFTER)
        .collect::<Vec<_>>();
    assert!(
        between.is_empty(),
        "{} of {} counts read during a reset from {BEFORE} to {AFTER} loans were neither: {:?}",
        between.len(),
        kept.len(),
        between.iter().take(20).collect::<Vec<_>>()
    );
}

/// A set outcome's removals are one write ([`spindle_kernel::Store::commit_all`]), so a count of
/// the rows read while it lands is the count before it or after it, never part of it.
#[test]
fn adversary_a_reader_counts_a_set_removal_whole() {
    const LENT: usize = 100;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("memory", state.path());
    let reader = host.reader();
    let phase = Arc::new(AtomicU8::new(IDLE));
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (phase, stop) = (Arc::clone(&phase), Arc::clone(&stop));
        std::thread::spawn(move || sample(&reader, &phase, &stop))
    };
    let mut rounds = 0;
    let started = Instant::now();
    while rounds < 60 && started.elapsed() < Duration::from_secs(60) {
        rounds += 1;
        for index in 0..LENT {
            let loan = format!("loan-{rounds}-{index}");
            submit(
                &mut host,
                "lending.loans.Lend",
                json!({"loan_id": loan, "borrower": "reader-0", "due": "2026-01-01T00:00:00Z"}),
            );
            submit(&mut host, "lending.loans.Return", json!({"loan_id": loan}));
        }
        phase.store(WATCHED, Ordering::SeqCst);
        submit(
            &mut host,
            "lending.loans.Purge",
            json!({"borrower": "reader-0"}),
        );
        phase.store(IDLE, Ordering::SeqCst);
    }
    stop.store(true, Ordering::SeqCst);
    let kept = sampler.join().expect("the sampler ends");
    assert!(
        !kept.is_empty(),
        "no count was read while a purge ran: the case observed nothing"
    );
    let between = kept
        .iter()
        .filter(|count| **count != LENT && **count != 0)
        .collect::<Vec<_>>();
    assert!(
        between.is_empty(),
        "{} of {} counts read while a purge removed {LENT} loans in one write were part of it: \
         {:?}",
        between.len(),
        kept.len(),
        between.iter().take(20).collect::<Vec<_>>()
    );
}

/// What the thread beside the writer does while a command is timed.
const NOTHING: u8 = 0;
const READING: u8 = 1;

/// How many commands each block times, how many blocks each way, and how long a switch of the
/// thread beside waits for the read it was running to end.
const BLOCK: usize = 5;
const BLOCKS: usize = 8;
const SWITCH: Duration = Duration::from_millis(60);

/// Times `Join` commands on `host`, in blocks that alternate between a thread beside it that
/// reads through `reader` with `read` and the same thread burning the processor and reading
/// nothing, so that the load of the machine falls on both alike. Answers the times beside no
/// read, then those beside a read.
fn joins(host: &mut Host, reader: &Reader, read: fn(&Reader)) -> (Vec<Duration>, Vec<Duration>) {
    let mode = Arc::new(AtomicU8::new(NOTHING));
    let stop = Arc::new(AtomicBool::new(false));
    let beside = {
        let (reader, mode, stop) = (reader.clone(), Arc::clone(&mode), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                if mode.load(Ordering::SeqCst) == READING {
                    read(&reader);
                } else {
                    let mut sum = 0_u64;
                    for value in 0..2_000_u64 {
                        sum = std::hint::black_box(sum.wrapping_add(value));
                    }
                }
            }
        })
    };
    let (mut idle, mut reading) = (Vec::new(), Vec::new());
    let mut member = 0;
    for block in 0..BLOCKS * 2 {
        let now = if block % 2 == 0 { NOTHING } else { READING };
        mode.store(now, Ordering::SeqCst);
        std::thread::sleep(SWITCH);
        for _ in 0..BLOCK {
            member += 1;
            let started = Instant::now();
            submit(
                host,
                "lending.loans.Join",
                json!({"member_id": format!("member-{member}"), "name": "N"}),
            );
            let taken = started.elapsed();
            if now == READING {
                reading.push(taken);
            } else {
                idle.push(taken);
            }
        }
    }
    stop.store(true, Ordering::SeqCst);
    beside.join().expect("the thread beside ends");
    (idle, reading)
}

fn read_members(reader: &Reader) {
    std::hint::black_box(reader.list(MEMBERS).expect("the members list"));
}

fn read_events(reader: &Reader) {
    std::hint::black_box(reader.published());
}

/// The mean of `durations` without the slowest tenth, which a loaded machine's scheduler
/// decides rather than the code.
fn trimmed_mean(mut durations: Vec<Duration>) -> Duration {
    durations.sort();
    let kept = &durations[..durations.len() - durations.len() / 10];
    kept.iter().sum::<Duration>() / u32::try_from(kept.len()).expect("a few times")
}

/// The command times beside a read, against those beside a thread that reads nothing: the
/// trimmed mean at most [`SLOWER`] times as long, plus [`SLACK`].
const SLOWER: u32 = 4;
const SLACK: Duration = Duration::from_millis(2);

fn assert_not_paid_for(what: &str, (idle, reading): (Vec<Duration>, Vec<Duration>)) {
    let count = reading.len();
    let (idle, reading) = (trimmed_mean(idle), trimmed_mean(reading));
    assert!(
        reading <= idle * SLOWER + SLACK,
        "{what}: a command takes {reading:?} (trimmed mean of {count}) while a thread reads \
         beside the host, {idle:?} while the same thread reads nothing: the writer pays for the \
         read"
    );
}

/// The [`spindle_kernel::Reading`] contract: "the writer never waits for a read beyond the moment
/// it takes to hand it the store's current state". The memory store holds every entity as one
/// value, so a read of the members (none) holds every loan, and a write of a member while it
/// reads copies all of them.
#[test]
fn adversary_reading_a_small_view_does_not_make_each_write_copy_the_whole_store() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("memory", state.path());
    host.reset_to(world("held", 20_000, "reader-0"))
        .expect("the world loads");
    let reader = host.reader();
    assert_not_paid_for(
        "joining a member beside a read of the members, with 20000 loans held",
        joins(&mut host, &reader, read_members),
    );
}

/// The same for `GET /api/events`: every event published since the last reset is one value its
/// readers share, so a command that publishes while a read holds it copies the whole history.
#[test]
fn adversary_reading_the_events_does_not_make_each_command_copy_every_event_published() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("memory", state.path());
    for index in 0..20_000 {
        submit(
            &mut host,
            "lending.loans.Lend",
            json!({"loan_id": format!("l-{index}"), "borrower": "b", "due": "2026-01-01T00:00:00Z"}),
        );
    }
    let reader = host.reader();
    assert!(reader.published().len() >= 20_000, "the history is held");
    assert_not_paid_for(
        "joining a member beside a read of the events, with 20000 events published",
        joins(&mut host, &reader, read_events),
    );
}

/// The file descriptors this process holds open on a path under `dir`, deleted ones included.
fn open_under(dir: &Path) -> Vec<String> {
    let dir = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned();
    std::fs::read_dir("/proc/self/fd")
        .expect("the process's descriptors are listed")
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .map(|target| target.to_string_lossy().into_owned())
        .filter(|target| target.starts_with(&dir))
        .collect()
}

/// How many descriptors a SQLite host may hold on its state once reads are done: the writer's
/// file, log and index, and a few reading connections kept for the next reads.
const KEPT: usize = 48;

/// A burst of reads on a SQLite store opens a connection for each read that runs at once
/// ([`spindle_kernel::SqliteStore`]'s reading handle). Each is kept for good, three descriptors
/// apiece, so one burst of N reads holds about 3N descriptors until the store is closed; at the
/// common soft limit of 1024 descriptors a burst of some 330 slow reads leaves none for the
/// listener's next connection or the writer.
#[test]
fn adversary_a_burst_of_reads_on_sqlite_does_not_keep_a_connection_per_read() {
    const READS: usize = 96;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("sqlite", state.path());
    host.reset_to(world("held", 1_000, "reader-0"))
        .expect("the world loads");
    let reader = host.reader();
    assert_eq!(reader.list(LOANS).expect("the loans list").len(), 1_000);
    let before = open_under(state.path()).len();
    let barrier = Arc::new(Barrier::new(READS));
    let readers = (0..READS)
        .map(|_| {
            let (reader, barrier) = (reader.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                reader.list(LOANS).expect("the loans list").len()
            })
        })
        .collect::<Vec<_>>();
    for read in readers {
        assert_eq!(read.join().expect("a read ends"), 1_000);
    }
    let after = open_under(state.path());
    assert!(
        after.len() <= KEPT,
        "after a burst of {READS} reads the process holds {} descriptors on the SQLite state \
         ({before} before the burst), each read's connection kept open",
        after.len()
    );
    drop(host);
}

/// A reset moves a SQLite store's files aside and opens it again; the reading handle of the
/// closed store must go with it, so that many resets leave no file of an earlier store open.
#[test]
fn adversary_many_resets_leave_no_earlier_sqlite_file_open() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("sqlite", state.path());
    let reader = host.reader();
    for round in 0..30 {
        submit(
            &mut host,
            "lending.loans.Lend",
            json!({"loan_id": format!("l-{round}"), "borrower": "b", "due": "2026-01-01T00:00:00Z"}),
        );
        assert_eq!(reader.list(LOANS).expect("the loans list").len(), 1);
        host.reset().expect("the reset empties the store");
    }
    let open = open_under(state.path());
    let deleted = open
        .iter()
        .filter(|target| target.ends_with("(deleted)"))
        .collect::<Vec<_>>();
    assert!(
        deleted.is_empty() && open.len() <= KEPT,
        "after 30 resets the process holds {} descriptors on the state, {} of them on files a \
         reset removed: {deleted:?}",
        open.len(),
        deleted.len()
    );
}

/// Read-your-writes on SQLite: once a command returns, a reader on another thread reads it, with
/// no wait, every time.
#[test]
fn adversary_a_reader_reads_a_command_once_it_returns_on_sqlite() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state");
    let mut host = host("sqlite", state.path());
    let reader = host.reader();
    for round in 0..40 {
        submit(
            &mut host,
            "lending.loans.Lend",
            json!({"loan_id": format!("l-{round}"), "borrower": "b", "due": "2026-01-01T00:00:00Z"}),
        );
        let read = {
            let reader = reader.clone();
            std::thread::spawn(move || reader.list(LOANS).expect("the loans list").len())
                .join()
                .expect("the read ends")
        };
        assert_eq!(read, round + 1, "round {round}: the read sees the command");
        assert_eq!(
            loans(&reader),
            round + 1,
            "round {round}: the count sees it"
        );
    }
}
