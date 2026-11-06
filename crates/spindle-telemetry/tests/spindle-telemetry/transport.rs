//! The transport: `spindle-telemetry/1` frames from an instance's counters to a collector.

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufReader;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use spindle_telemetry::{
    Counters, Delivery, Endpoint, FrameReader, Outcome, Readings, Record, SenderOptions, Telemetry,
    Transmitter, catalogue, read_resources,
};

const COMMAND: &str = "rally.table.OpenMatch";
const OPENED: &str = "command.rally.table.OpenMatch.opened";

fn rally() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess")
}

fn counters() -> Arc<Counters> {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    Arc::new(Counters::new(
        catalogue(&loaded.ir, "rally-table").expect("rally-table exists"),
    ))
}

fn open(counters: &Counters, times: u64) {
    for _ in 0..times {
        counters.record(Record::Command {
            command: COMMAND,
            outcome: Outcome::Declared("opened"),
            elapsed: Duration::from_micros(120),
        });
    }
}

fn options(socket: &Path, period: Duration) -> SenderOptions {
    SenderOptions::new(
        "rally-table-0",
        Endpoint::unix(socket).expect("a Unix endpoint"),
        period,
    )
}

/// A directory under the target directory, and a collector socket in it named relative to the
/// working directory of the test (the crate), so the path fits the 108 bytes of `sun_path`
/// however deep the tree lies.
fn directory() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace");
    let inside = dir
        .path()
        .canonicalize()
        .expect("the directory")
        .strip_prefix(&root)
        .expect("the target directory is in the workspace")
        .to_path_buf();
    let socket = Path::new("../..").join(inside).join("telemetry.sock");
    (dir, socket)
}

fn totals(frame: &spindle_telemetry::Frame) -> BTreeMap<String, u64> {
    frame
        .counters
        .iter()
        .map(|counter| (counter.name.clone(), counter.value))
        .collect()
}

#[test]
fn a_lost_frame_loses_no_count() {
    let (_dir, socket) = directory();
    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let counters = counters();
    let mut transmitter =
        Transmitter::new(counters.clone(), options(&socket, Duration::from_secs(1)));

    open(&counters, 2);
    assert_eq!(transmitter.send(), Delivery::Sent);
    let (stream, _) = listener.accept().expect("the instance connects");
    let mut collector = FrameReader::new(BufReader::new(stream));
    let first = collector.next_frame().expect("a read").expect("frame 1");
    assert_eq!(first.sequence, 0);
    assert_eq!(totals(&first)[OPENED], 2);

    open(&counters, 3);
    assert_eq!(transmitter.send(), Delivery::Sent);
    let lost = collector.next_frame().expect("a read").expect("frame 2");
    assert_eq!(lost.sequence, 1);
    drop(lost);

    open(&counters, 4);
    assert_eq!(transmitter.send(), Delivery::Sent);
    let third = collector.next_frame().expect("a read").expect("frame 3");
    assert_eq!(third.sequence, 2);

    let snapshot = counters.snapshot();
    let expected: BTreeMap<String, u64> = snapshot
        .counters
        .iter()
        .map(|counter| (counter.name.clone(), counter.value))
        .collect();
    assert_eq!(totals(&third), expected);
    assert_eq!(totals(&third)[OPENED], 9);
    assert_eq!(third.histograms, snapshot.histograms);
}

#[test]
fn a_missing_collector_drops_frames_and_counts_the_drops() {
    let (_dir, socket) = directory();
    let counters = counters();
    let sender = spindle_telemetry::Sender::start(
        counters.clone(),
        options(&socket, Duration::from_millis(20)),
    )
    .expect("the sender starts");

    let mut slowest = Duration::ZERO;
    for _ in 0..1_000 {
        let started = Instant::now();
        open(&counters, 1);
        slowest = slowest.max(started.elapsed());
    }
    assert!(
        slowest < Duration::from_millis(5),
        "recording waited {slowest:?} while nobody collects"
    );

    let mut steps = 0;
    while sender.dropped() < 3 {
        steps += 1;
        assert!(
            steps < 500,
            "the sender dropped {} frames",
            sender.dropped()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(sender.sent(), 0);

    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let (stream, _) = listener.accept().expect("the instance connects");
    let mut collector = FrameReader::new(BufReader::new(stream));
    let frame = collector.next_frame().expect("a read").expect("a frame");
    assert!(
        frame.dropped_frames >= 3,
        "the frame reports {} drops",
        frame.dropped_frames
    );
    assert!(
        frame.sequence >= frame.dropped_frames,
        "a dropped frame used its sequence number: {} after {} drops",
        frame.sequence,
        frame.dropped_frames
    );
    assert_eq!(totals(&frame)[OPENED], 1_000);
    sender.stop();
}

#[test]
fn a_frame_validates_against_the_sample_schema() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ess/generated/jsonschema/schema/types/spindle.observe.Sample.schema.json");
    let source = std::fs::read_to_string(&path).expect("the Sample schema");
    let root = "spindle.observe.Sample.document";
    let bundle = schema_contract::bundle::import_document(
        &source,
        root,
        &BTreeSet::new(),
        schema_contract::bundle::Dialect::Draft202012,
    )
    .expect("the schema imports");
    let validator = bundle.validator(root).expect("the root compiles");

    let (_dir, socket) = directory();
    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let counters = counters();
    open(&counters, 2);
    let mut transmitter = Transmitter::new(
        counters.clone(),
        options(&socket, Duration::from_millis(250)).with_readings(Readings::Process),
    );
    assert_eq!(transmitter.send(), Delivery::Sent);
    let (stream, _) = listener.accept().expect("the instance connects");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut BufReader::new(stream), &mut line).expect("a line");
    let value: serde_json::Value = serde_json::from_str(&line).expect("the frame is JSON");

    let errors = validator.validate(&value);
    assert!(
        errors.is_empty(),
        "the frame is not a Sample: {errors:?}\n{line}"
    );
    assert_eq!(value["instance_id"], "rally-table-0");
    assert_eq!(value["period_ms"], 250);
    assert!(
        value["counters"]
            .as_array()
            .is_some_and(|counters| !counters.is_empty()),
        "the frame carries the counters: {line}"
    );

    let mut wrong = value.clone();
    wrong["unexpected"] = serde_json::json!(1);
    assert!(
        !validator.validate(&wrong).is_empty(),
        "the validator refuses a field the Sample does not declare"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn the_frame_carries_the_guest_resource_readings() {
    let mut steps = 0u64;
    let mut spin = 0u64;
    loop {
        let reading = read_resources(Readings::Process).expect("the process readings");
        if reading.cpu_seconds > 0.0 {
            assert!(reading.memory_bytes > 0, "{reading:?}");
            break;
        }
        steps += 1;
        assert!(steps < 1_000, "no CPU time after {steps} reads");
        for value in 0..200_000u64 {
            spin = spin.wrapping_mul(31).wrapping_add(value);
        }
        std::hint::black_box(spin);
    }
    let machine = read_resources(Readings::Machine).expect("the machine readings");
    assert!(machine.cpu_seconds > 0.0, "{machine:?}");
    assert!(machine.memory_bytes > 0, "{machine:?}");

    let (_dir, socket) = directory();
    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let mut transmitter = Transmitter::new(
        counters(),
        options(&socket, Duration::from_secs(1)).with_readings(Readings::Machine),
    );
    assert_eq!(transmitter.send(), Delivery::Sent);
    let (stream, _) = listener.accept().expect("the instance connects");
    let frame = FrameReader::new(BufReader::new(stream))
        .next_frame()
        .expect("a read")
        .expect("a frame");
    assert!(frame.resources.cpu_seconds > 0.0, "{:?}", frame.resources);
    assert!(frame.resources.memory_bytes > 0, "{:?}", frame.resources);
}

#[test]
fn an_endpoint_reads_back_as_it_is_written() {
    for text in ["unix:/run/spindle/telemetry.sock", "vsock:2:7341"] {
        let endpoint: Endpoint = text.parse().expect("an endpoint");
        assert_eq!(endpoint.to_string(), text);
    }
    for text in [
        "",
        "tcp:127.0.0.1:1",
        "unix:",
        "vsock:2",
        "vsock:x:1",
        "vsock:2:70000000000",
    ] {
        assert!(text.parse::<Endpoint>().is_err(), "{text:?} is refused");
    }
}

/// `sun_path` is 108 bytes on Linux and 104 on macOS; the longest path is one less, for the NUL,
/// and a path of that length is accepted while one byte more is refused.
#[test]
fn the_longest_unix_path_is_the_platform_sun_path_less_its_nul() {
    let expected = if cfg!(target_os = "linux") {
        107
    } else if cfg!(target_vendor = "apple") {
        103
    } else {
        spindle_telemetry::MAX_UNIX_PATH
    };
    assert_eq!(spindle_telemetry::MAX_UNIX_PATH, expected);
    let longest = format!("/{}", "s".repeat(spindle_telemetry::MAX_UNIX_PATH - 1));
    assert!(Endpoint::unix(&longest).is_ok(), "{} bytes", longest.len());
    assert!(
        Endpoint::unix(format!("{longest}s")).is_err(),
        "{} bytes",
        longest.len() + 1
    );
}

/// Off Linux a vsock endpoint still reads back, and connecting to it, listening and reading
/// resources are each refused by name.
#[cfg(not(target_os = "linux"))]
#[test]
fn vsock_and_proc_readings_are_refused_off_linux() {
    use spindle_telemetry::vsock::{REFUSAL, VsockListener, VsockStream};

    let endpoint: Endpoint = "vsock:2:7341".parse().expect("an endpoint");
    let connected = endpoint.connect(Duration::from_millis(50));
    assert!(
        connected
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::Unsupported
                && error.to_string() == REFUSAL),
        "{connected:?}"
    );
    let stream = VsockStream::connect_within(2, 7341, Duration::from_millis(50));
    assert!(
        stream
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::Unsupported),
        "{stream:?}"
    );
    let listener = VsockListener::bind_any_port();
    assert!(
        listener
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::Unsupported),
        "{listener:?}"
    );
    for readings in [Readings::Process, Readings::Machine] {
        let read = read_resources(readings);
        assert!(
            read.as_ref()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::Unsupported
                    && error.to_string() == spindle_telemetry::READINGS_REFUSAL),
            "{read:?}"
        );
    }
}
