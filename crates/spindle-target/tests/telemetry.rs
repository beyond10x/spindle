//! The process launcher gives an instance its telemetry endpoint.

use std::io::BufReader;
use std::net::SocketAddr;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spindle_target::{Artifact, LaunchPlan, Launcher, ProcessLauncher, Resources};
use spindle_telemetry::{ENV_TELEMETRY, Endpoint, FrameReader};

/// `dir`, named relative to the working directory of the test and of the instances it starts
/// (the crate), so a socket path in it fits the 108 bytes of `sun_path` however deep the tree
/// lies.
fn relative_to_the_crate(dir: &Path) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace");
    let inside = dir
        .canonicalize()
        .expect("the directory")
        .strip_prefix(&root)
        .expect("the target directory is in the workspace")
        .to_path_buf();
    Path::new("../..").join(inside)
}

fn plan(instance_id: &str) -> LaunchPlan {
    LaunchPlan {
        instance_id: instance_id.to_owned(),
        component: "rally-table".to_owned(),
        settings: vec![("mode".to_owned(), "telemetry".to_owned())],
        artifact: Artifact::HostBinary {
            path: PathBuf::from(env!("CARGO_BIN_EXE_spindle-target-fixture")),
            args: Vec::new(),
        },
        request: Resources::default(),
        listen: SocketAddr::from(([127, 0, 0, 1], 7100)),
        peers: Vec::new(),
    }
}

#[test]
fn a_process_instance_reports_over_a_unix_socket() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let socket = relative_to_the_crate(dir.path()).join("telemetry.sock");
    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let mut launcher = ProcessLauncher::new(dir.path().join("consoles"))
        .with_telemetry(&socket)
        .expect("the endpoint fits");

    let mut instance = launcher
        .launch(&plan("rally-table-0"))
        .expect("the instance starts");
    listener
        .set_nonblocking(true)
        .expect("a non-blocking listener");
    let mut steps = 0;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                steps += 1;
                assert!(
                    steps < 1_000,
                    "the instance never connected; console:\n{}",
                    std::fs::read_to_string(instance.console()).unwrap_or_default()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept: {error}"),
        }
    };
    stream.set_nonblocking(false).expect("a blocking stream");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("a read timeout");
    let mut collector = FrameReader::new(BufReader::new(stream));
    let mut sequences = Vec::new();
    for _ in 0..3 {
        let frame = collector
            .next_frame()
            .expect("a read")
            .expect("a frame before the instance ends");
        assert_eq!(frame.instance_id, "rally-table-0");
        assert!(
            frame.counters.iter().any(|counter| counter.value > 0),
            "the instance's own counters arrive: {:?}",
            frame.counters
        );
        sequences.push(frame.sequence);
    }
    instance.stop().expect("the instance stops");

    assert!(
        sequences.windows(2).all(|pair| pair[0] < pair[1]),
        "sequence numbers increase: {sequences:?}"
    );
    let console = std::fs::read_to_string(instance.console()).expect("the console");
    let expected = format!(
        "env {ENV_TELEMETRY}={}",
        Endpoint::unix(socket.clone()).expect("a Unix endpoint")
    );
    assert!(
        console.lines().any(|line| line == expected),
        "the launcher sets {expected}:\n{console}"
    );
}

#[test]
fn a_socket_path_an_instance_cannot_connect_to_is_refused_by_the_launcher() {
    use std::os::unix::ffi::OsStrExt;

    let long = PathBuf::from(format!("/{}/c.sock", "d".repeat(120)));
    let non_utf8 = PathBuf::from(std::ffi::OsStr::from_bytes(b"/run/spindle-\xff/c.sock"));
    for (why, path) in [("too long", long), ("not UTF-8", non_utf8)] {
        match ProcessLauncher::new("consoles").with_telemetry(&path) {
            Err(spindle_target::LaunchError::InvalidPlan(message)) => {
                assert!(message.contains("Unix socket path"), "{why}: {message}")
            }
            other => panic!("{why}: {path:?} is not refused: {other:?}"),
        }
    }
}
