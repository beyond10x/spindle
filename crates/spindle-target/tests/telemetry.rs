//! The process launcher gives an instance its telemetry endpoint.

use std::io::BufReader;
use std::net::SocketAddr;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::Duration;

use spindle_target::{Artifact, LaunchPlan, Launcher, ProcessLauncher, Resources};
use spindle_telemetry::{ENV_TELEMETRY, Endpoint, FrameReader};

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
    // A socket path must fit the 108 bytes of `sun_path`; a deep target directory does not.
    let short = tempfile::tempdir().expect("a directory");
    let socket = if dir.path().join("telemetry.sock").as_os_str().len() < 100 {
        dir.path().join("telemetry.sock")
    } else {
        short.path().join("telemetry.sock")
    };
    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let mut launcher = ProcessLauncher::new(dir.path().join("consoles")).with_telemetry(&socket);

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
    let expected = format!("env {ENV_TELEMETRY}={}", Endpoint::Unix(socket.clone()));
    assert!(
        console.lines().any(|line| line == expected),
        "the launcher sets {expected}:\n{console}"
    );
}
