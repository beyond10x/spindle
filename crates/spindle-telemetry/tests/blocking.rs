//! Recording never blocks a command: it allocates nothing, takes no lock, and loses no count
//! while other threads record and read. Reading an oversized frame holds no more memory than the
//! frame limit.
//!
//! This binary counts the allocations of each thread, and the bytes each thread holds, so a
//! thread's own count is exact whatever else runs.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Map, json};
use spindle_kernel::{Change, Event, Executed, Row, Store, StoreError, Stored};
use spindle_telemetry::{
    Carried, Counters, CountingStore, FRAME_LIMIT, FrameReader, Outcome, Record, Sink, Telemetry,
    Timed, catalogue,
};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static HELD: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
}

/// Notes that this thread holds `gained` bytes more and `lost` bytes fewer. Memory freed on
/// another thread than the one that took it is not seen, so a thread's count may only be read
/// while it alone takes and frees what it measures.
fn hold(gained: usize, lost: usize) {
    HELD.with(|held| {
        let now = (held.get() + gained).saturating_sub(lost);
        held.set(now);
        PEAK.with(|peak| peak.set(peak.get().max(now)));
    });
}

// SAFETY: every call is forwarded to the system allocator unchanged; the thread-local counters
// are const-initialised and have no destructor, so touching them never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
        hold(layout.size(), 0);
        // SAFETY: the caller's contract is passed on unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        hold(0, layout.size());
        // SAFETY: the caller's contract is passed on unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
        hold(size, layout.size());
        // SAFETY: the caller's contract is passed on unchanged.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

/// The bytes this thread holds now, and the most it held since the last call.
fn held() -> (usize, usize) {
    let now = HELD.with(Cell::get);
    let peak = PEAK.with(|peak| peak.replace(now));
    (now, peak)
}

/// `total` bytes of `x` with no newline, counting what was read of them.
struct Endless {
    total: usize,
    served: Rc<Cell<usize>>,
}

impl Read for Endless {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = buffer.len().min(self.total - self.served.get());
        buffer[..count].fill(b'x');
        self.served.set(self.served.get() + count);
        Ok(count)
    }
}

/// The capacity of a default `BufReader`: what one read past the limit may take.
const ONE_READ: usize = 8 * 1024;

#[test]
fn an_oversized_frame_is_refused_without_holding_more_than_the_limit() {
    let served = Rc::new(Cell::new(0));
    let mut reader = FrameReader::new(BufReader::new(Endless {
        total: 8 * FRAME_LIMIT,
        served: Rc::clone(&served),
    }));
    let (before, _) = held();
    let error = reader
        .next_frame()
        .expect_err("a line longer than the limit is refused");
    let (_, peak) = held();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(
        served.get() <= FRAME_LIMIT + 1 + ONE_READ,
        "the reader read {} bytes of a line with no newline, past the limit of {FRAME_LIMIT} and \
         one read",
        served.get()
    );
    assert!(
        peak - before <= FRAME_LIMIT + 1 + ONE_READ,
        "reading an oversized line held {} bytes more, past the limit of {FRAME_LIMIT} and one \
         read",
        peak - before
    );
}

/// A store that keeps nothing, so a commit through [`CountingStore`] measures the counting alone.
struct Nowhere;

impl Store for Nowhere {
    fn load(&mut self, _: &str, _: &serde_json::Value) -> Result<Option<Stored>, StoreError> {
        Ok(None)
    }

    fn commit(&mut self, _: &Change) -> Result<(), StoreError> {
        Ok(())
    }

    fn remove(&mut self, _: &spindle_kernel::Removal) -> Result<(), StoreError> {
        Ok(())
    }

    fn scan(&mut self, _: &str) -> Result<spindle_kernel::Scan, StoreError> {
        Ok(spindle_kernel::Scan::default())
    }
}

const ROUNDS: u64 = 10_000;
const WRITERS: u64 = 4;
const READS: usize = 100_000;

#[test]
fn recording_never_blocks_a_command() {
    let rally = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess");
    let loaded = spindle_spec::load(&rally).expect("rally compiles");
    let counters = Arc::new(Counters::new(
        catalogue(&loaded.ir, "rally-table").expect("rally-table exists"),
    ));
    let sink = Sink::new(counters.clone());
    let mut store = CountingStore::new(Nowhere, sink.clone());
    let mut payload = Map::new();
    payload.insert("match_id".to_owned(), json!("m1"));
    payload.insert("ball_x".to_owned(), json!(4));
    let change = Change {
        entity: "rally.table.Match".to_owned(),
        identity: json!("m1"),
        expected: Some(1),
        row: Row {
            state: "Playing".to_owned(),
            fields: [("ball_x".to_owned(), json!(4))].into_iter().collect(),
        },
        events: vec![Event {
            name: "rally.table.BallMoved".to_owned(),
            payload,
        }],
        scanned: Vec::new(),
    };
    let executed = Ok(Executed {
        outcome: "opened".to_owned(),
        events: Vec::new(),
        error: None,
        response: None,
    });
    let failed = Err(spindle_kernel::ExecuteError::InvalidInput("x".to_owned()));

    let before = allocations();
    for _ in 0..ROUNDS {
        sink.executed(
            "rally.table.OpenMatch",
            &executed,
            Duration::from_micros(42),
        );
        sink.executed("rally.table.OpenMatch", &failed, Duration::from_micros(7));
        store.commit(&change).expect("nowhere commits");
        sink.carried("west-watches", Carried::Sent);
        sink.carried("west-moves", Carried::Delivered);
        sink.timed("tick", Timed::Fired, 1);
        sink.carried("a-binding-the-ir-lacks", Carried::Dropped);
    }
    assert_eq!(
        allocations() - before,
        0,
        "recording allocated on the command path"
    );
    assert_eq!(
        counters.value("command.rally.table.OpenMatch.opened"),
        Some(ROUNDS)
    );
    assert_eq!(
        counters.value("command.rally.table.OpenMatch.failed.invalid_input"),
        Some(ROUNDS)
    );
    assert_eq!(counters.value("store.rows_written"), Some(ROUNDS));
    assert!(counters.value("store.bytes_written") > Some(ROUNDS));
    assert_eq!(
        counters.value("event.rally.table.BallMoved.appended"),
        Some(ROUNDS)
    );
    assert_eq!(counters.value("binding.west-watches.sent"), Some(ROUNDS));
    assert_eq!(counters.value("timer.tick.fired"), Some(ROUNDS));
    assert_eq!(counters.uncatalogued(), ROUNDS);

    let reading = Arc::new(AtomicBool::new(true));
    let reader = {
        let counters = counters.clone();
        let reading = reading.clone();
        std::thread::spawn(move || {
            let mut last = 0;
            for _ in 0..READS {
                if !reading.load(Ordering::Relaxed) {
                    break;
                }
                let seen = counters.value("binding.east-watches.sent").unwrap_or(0);
                assert!(seen >= last, "a counter went back from {last} to {seen}");
                last = seen;
                let _ = counters.snapshot();
            }
        })
    };
    let writers = (0..WRITERS)
        .map(|_| {
            let counters = counters.clone();
            std::thread::spawn(move || {
                let before = allocations();
                for _ in 0..ROUNDS {
                    counters.record(Record::Carried {
                        binding: "east-watches",
                        carried: Carried::Sent,
                    });
                    counters.record(Record::Command {
                        command: "rally.table.Tick",
                        outcome: Outcome::Declared("not-a-declared-outcome"),
                        elapsed: Duration::ZERO,
                    });
                }
                allocations() - before
            })
        })
        .collect::<Vec<_>>();
    for writer in writers {
        assert_eq!(writer.join().expect("a writer finishes"), 0);
    }
    reading.store(false, Ordering::Relaxed);
    reader.join().expect("the reader finishes");
    assert_eq!(
        counters.value("binding.east-watches.sent"),
        Some(WRITERS * ROUNDS),
        "no increment is lost while other threads record and read"
    );
    assert_eq!(counters.uncatalogued(), ROUNDS + WRITERS * ROUNDS);
}
