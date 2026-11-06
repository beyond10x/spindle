//! Adversary cases for the telemetry transport.

use std::ffi::OsStr;
use std::io::BufReader;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use spindle_telemetry::{
    Catalogue, Counter, Counters, Delivery, Endpoint, FrameReader, Key, Kind, Outcome, Record,
    Sender, SenderOptions, Source, Telemetry, Transmitter,
};

fn counters() -> Arc<Counters> {
    let counters = Arc::new(Counters::new(Catalogue {
        component: "adv".to_owned(),
        counters: vec![Counter {
            name: "command.adv.Ping.ok".to_owned(),
            kind: Kind::Counter,
            source: Source::Kernel,
            key: Key::Outcome {
                command: "adv.Ping".to_owned(),
                outcome: "ok".to_owned(),
            },
        }],
    }));
    counters.record(Record::Command {
        command: "adv.Ping",
        outcome: Outcome::Declared("ok"),
        elapsed: Duration::from_micros(5),
    });
    counters
}

/// A directory under the target directory, and a socket in it named relative to the test's
/// working directory (the crate), so the path fits `sun_path` however deep the tree is.
fn directory() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("the crate");
    assert_eq!(
        std::env::current_dir()
            .expect("a working directory")
            .canonicalize()
            .expect("the working directory"),
        manifest,
        "tests run in the crate directory"
    );
    let root = manifest
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
    let socket = Path::new("../..").join(inside).join("c.sock");
    assert!(socket.as_os_str().len() < 100, "{}", socket.display());
    (dir, socket)
}

/// A collector that listens with a backlog of `backlog` and never accepts.
fn stuck_listener(path: &Path, backlog: i32) -> std::os::fd::OwnedFd {
    use std::os::fd::FromRawFd;
    // SAFETY: plain socket/bind/listen calls on a fresh descriptor.
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        assert!(fd >= 0, "socket");
        let mut address: libc::sockaddr_un = std::mem::zeroed();
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (slot, byte) in address.sun_path.iter_mut().zip(path.as_os_str().as_bytes()) {
            *slot = *byte as libc::c_char;
        }
        let length = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
        assert_eq!(
            libc::bind(fd, (&raw const address).cast(), length),
            0,
            "bind"
        );
        assert_eq!(libc::listen(fd, backlog), 0, "listen");
        std::os::fd::OwnedFd::from_raw_fd(fd)
    }
}

/// A collector that is alive but not accepting (its backlog full) must not stop the sender from
/// stopping: `Sender::stop` promises to wait at most one write timeout.
#[test]
fn stopping_a_sender_whose_collector_does_not_accept_returns() {
    let (_dir, socket) = directory();
    let _listener = stuck_listener(&socket, 0);
    let _filler = UnixStream::connect(&socket).expect("the one backlog slot");

    let options = SenderOptions::new(
        "adv-0",
        Endpoint::Unix(socket.clone()),
        Duration::from_millis(50),
    );
    let sender = Sender::start(counters(), options).expect("the sender starts");
    std::thread::sleep(Duration::from_millis(300));

    let (done, stopped) = mpsc::channel();
    std::thread::spawn(move || {
        sender.stop();
        let _ = done.send(());
    });
    assert!(
        stopped.recv_timeout(Duration::from_secs(3)).is_ok(),
        "Sender::stop did not return within 3 s of a 50 ms period: the sender thread is blocked \
         in connect(2) on a collector whose backlog is full"
    );
}

/// A collector that accepts once and then never reads: the sender drops, reconnects into the
/// backlog, and once that is full `Transmitter::send` must still return.
#[test]
fn a_collector_that_accepts_once_and_never_reads_does_not_block_a_send() {
    let (_dir, socket) = directory();
    let listener = stuck_listener(&socket, 4);
    let options = SenderOptions::new(
        "adv-0",
        Endpoint::Unix(socket.clone()),
        Duration::from_millis(1),
    );
    let (progress, sends) = mpsc::channel::<(u64, Delivery)>();
    std::thread::spawn(move || {
        let mut transmitter = Transmitter::new(counters(), options);
        for step in 0..200_000u64 {
            let delivery = transmitter.send();
            if progress.send((step, delivery)).is_err() {
                return;
            }
        }
    });
    let (first, _) = sends
        .recv_timeout(Duration::from_secs(5))
        .expect("the first send returns");
    assert_eq!(first, 0);
    // SAFETY: a plain accept on the listener; the connection is held open and never read.
    let accepted = unsafe {
        libc::accept(
            std::os::fd::AsRawFd::as_raw_fd(&listener),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert!(accepted >= 0, "the collector accepts once");

    let mut last = 0;
    let mut dropped = 0;
    loop {
        match sends.recv_timeout(Duration::from_secs(3)) {
            Ok((step, delivery)) => {
                last = step;
                dropped += u64::from(delivery == Delivery::Dropped);
                if step == 199_999 {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!(
                "Transmitter::send blocked for 3 s with a 1 ms period after {last} sends \
                 ({dropped} dropped): a collector that accepted once and stopped reading hangs \
                 the sender thread"
            ),
        }
    }
}

/// `sun_path` holds 108 bytes. A launcher can hand out a longer path, and the instance then
/// drops every frame for its whole life without saying why; the endpoint should be refused
/// where it is read.
#[test]
fn an_overlong_unix_path_is_refused_when_read_from_the_environment() {
    let long = format!("unix:/{}/c.sock", "d".repeat(120));
    let parsed = long.parse::<Endpoint>();
    let options = SenderOptions::from_lookup(|name| match name {
        "SPINDLE_TELEMETRY" => Some(long.clone()),
        "SPINDLE_INSTANCE_ID" => Some("adv-0".to_owned()),
        _ => None,
    });
    assert!(
        parsed.is_err() || options.is_err(),
        "a {}-byte Unix socket path is accepted ({parsed:?}); connect(2) refuses it on every \
         frame",
        long.len() - "unix:".len()
    );
}

/// The process launcher writes the endpoint with `Display`, which cannot carry a non-UTF-8 path:
/// such a path is refused by name, never rewritten into another socket's.
#[test]
fn a_unix_endpoint_with_a_non_utf8_path_is_refused() {
    let path = PathBuf::from(OsStr::from_bytes(b"/run/spindle-\xff/c.sock"));
    let refused = Endpoint::unix(path.clone());
    assert!(
        refused
            .as_ref()
            .is_err_and(|error| error.0.contains("not UTF-8")),
        "{refused:?}"
    );
    assert!(Endpoint::Unix(path.clone()).check().is_err());
    let options = SenderOptions::new("adv-0", Endpoint::Unix(path), Duration::from_millis(50));
    let started = Sender::start(counters(), options);
    assert!(
        started
            .as_ref()
            .is_err_and(|error| error.to_string().contains("not UTF-8")),
        "the sender starts on an endpoint it cannot name: {:?}",
        started.map(|_| ())
    );
}

/// A drop and a reconnect: every frame uses one sequence number and the drops are counted once.
#[test]
fn a_reconnect_after_the_collector_restarts_counts_each_drop_once() {
    let (_dir, socket) = directory();
    let listener = UnixListener::bind(&socket).expect("the collector listens");
    let options = SenderOptions::new(
        "adv-0",
        Endpoint::Unix(socket.clone()),
        Duration::from_millis(200),
    );
    let mut transmitter = Transmitter::new(counters(), options);
    assert_eq!(transmitter.send(), Delivery::Sent);
    let (stream, _) = listener.accept().expect("the instance connects");
    let first = FrameReader::new(BufReader::new(&stream))
        .next_frame()
        .expect("a read")
        .expect("a frame");
    assert_eq!((first.sequence, first.dropped_frames), (0, 0));
    drop(stream);
    drop(listener);
    std::fs::remove_file(&socket).expect("the socket is removed");

    let mut drops = 0;
    for _ in 0..3 {
        if transmitter.send() == Delivery::Dropped {
            drops += 1;
        }
    }
    assert!(
        drops >= 2,
        "only {drops} of 3 frames to a gone collector dropped"
    );

    let listener = UnixListener::bind(&socket).expect("the collector listens again");
    assert_eq!(transmitter.send(), Delivery::Sent);
    let (stream, _) = listener.accept().expect("the instance reconnects");
    let next = FrameReader::new(BufReader::new(stream))
        .next_frame()
        .expect("a read")
        .expect("a frame");
    assert_eq!(next.sequence, 4);
    assert_eq!(next.dropped_frames, drops);
    assert_eq!(transmitter.dropped(), drops);
}
