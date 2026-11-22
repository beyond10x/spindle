//! The collector: instances' telemetry connections accepted and their frames read, within limits.

use std::cell::Cell;
use std::io::{self, BufReader, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use spindle_telemetry::{DEFAULT_PERIOD, Frame, FrameReader};

/// How often the collector looks for connections and the stop flag.
const POLL: Duration = Duration::from_millis(10);

/// How many send periods a connection may go without a complete frame.
const DEADLINE_PERIODS: u32 = 5;

/// Reader threads beyond one per instance: a sender that dropped a frame reconnects, and the
/// reader of its old connection may not have seen its end yet.
const SPARE_READERS: usize = 8;

/// A frame and the process id of the peer that sent it.
pub(crate) type Collected = (Option<u32>, Frame);

/// What one collector may hold.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Reader threads at once.
    pub readers: usize,
    /// Frames read and not yet taken by the supervisor.
    pub queue: usize,
    /// How long a connection may go without a complete frame before its first.
    pub deadline: Duration,
    /// The longest deadline a frame's period may stretch it to.
    pub longest: Duration,
}

impl Limits {
    /// The limits of a control process with `instances` instances: one reader per instance and
    /// [`SPARE_READERS`] more, two queued frames per reader, and [`DEADLINE_PERIODS`] periods of
    /// [`DEFAULT_PERIOD`] for a connection's first frame, stretched by a longer period a frame
    /// names up to five minutes.
    ///
    /// The collector then holds at most a line buffer of
    /// [`FRAME_LIMIT`](spindle_telemetry::FRAME_LIMIT) and one read per reader, and the parsed
    /// frames of a full queue.
    pub fn for_instances(instances: usize) -> Self {
        let readers = instances + SPARE_READERS;
        Self {
            readers,
            queue: readers * 2,
            deadline: DEFAULT_PERIOD * DEADLINE_PERIODS,
            longest: Duration::from_secs(300),
        }
    }
}

/// What a collector counts, readable while it runs.
#[derive(Debug, Default)]
pub(crate) struct Counts {
    readers: AtomicUsize,
    dropped: AtomicU64,
}

impl Counts {
    /// Reader threads running now.
    #[cfg(test)]
    pub fn readers(&self) -> usize {
        self.readers.load(Ordering::SeqCst)
    }

    /// Frames dropped because the supervisor's queue was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
    }
}

/// One running reader, counted in [`Counts::readers`] until it is dropped.
struct Reader(Arc<Counts>);

impl Reader {
    /// A reader, if fewer than `limit` run.
    fn admit(counts: &Arc<Counts>, limit: usize) -> Option<Self> {
        counts
            .readers
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |running| {
                (running < limit).then_some(running + 1)
            })
            .ok()
            .map(|_| Self(Arc::clone(counts)))
    }

    fn counts(&self) -> &Counts {
        &self.0
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.0.readers.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Accepts instances' telemetry connections until `stop`, reading each on a thread of its own.
///
/// A connection accepted while [`Limits::readers`] readers run is closed at once.
pub(crate) fn collect(
    listener: &UnixListener,
    frames: &mpsc::SyncSender<Collected>,
    limits: Limits,
    counts: &Arc<Counts>,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let Some(reader) = Reader::admit(counts, limits.readers) else {
                    drop(stream);
                    continue;
                };
                let frames = frames.clone();
                let _ = std::thread::Builder::new()
                    .name("spindle-control-frames".to_owned())
                    .spawn(move || read(stream, &frames, limits, reader.counts()));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL);
            }
            Err(_) => std::thread::sleep(POLL),
        }
    }
}

/// A connection read against a deadline for its next complete frame: each read waits at most
/// what is left of it, so a peer that drips bytes is closed as one that sends none.
struct Deadline {
    stream: UnixStream,
    until: Rc<Cell<Instant>>,
}

impl Read for Deadline {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let left = self.until.get().saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buffer)
    }
}

/// How long a connection whose last frame named `period` may take for its next: five periods,
/// at least [`Limits::deadline`] and at most [`Limits::longest`].
fn deadline(period: Duration, limits: Limits) -> Duration {
    period
        .saturating_mul(DEADLINE_PERIODS)
        .min(limits.longest)
        .max(limits.deadline)
}

/// Reads frames from one connection until it closes, sends something that is not a frame, sends
/// a line longer than [`spindle_telemetry::FRAME_LIMIT`] or lets its deadline pass, each with the
/// process id of the connection's peer.
///
/// The connection is bound to the instance id of its first frame: a later frame naming another id
/// is dropped. Which connection may speak for an instance is the supervisor's check: the peer must
/// be in the instance's process group. A frame the supervisor's full queue cannot take is dropped
/// and counted ([`Counts::dropped`]); the reader never waits for room.
fn read(stream: UnixStream, frames: &mpsc::SyncSender<Collected>, limits: Limits, counts: &Counts) {
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let peer = peer_pid(&stream);
    let until = Rc::new(Cell::new(Instant::now() + limits.deadline));
    let mut reader = FrameReader::new(BufReader::new(Deadline {
        stream,
        until: Rc::clone(&until),
    }));
    let mut bound: Option<String> = None;
    while let Ok(Some(frame)) = reader.next_frame() {
        until.set(Instant::now() + deadline(Duration::from_millis(frame.period_ms), limits));
        let id = bound.get_or_insert_with(|| frame.instance_id.clone());
        if frame.instance_id != *id {
            continue;
        }
        match frames.try_send((peer, frame)) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                counts.dropped.fetch_add(1, Ordering::SeqCst);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return,
        }
    }
}

/// The process id of the peer of a Unix stream: `SO_PEERCRED` on Linux, `LOCAL_PEERPID` on
/// Apple systems; `None` where neither exists, so no frame counts for an instance in a process
/// group of its own.
#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    rustix::net::sockopt::socket_peercred(stream)
        .ok()
        .map(|credentials| credentials.pid.as_raw_nonzero().get().unsigned_abs())
}

#[cfg(target_vendor = "apple")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd as _;
    let mut pid: libc::pid_t = 0;
    let mut length = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: `pid` and `length` are valid for writing for the duration of the call.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast(),
            &raw mut length,
        )
    };
    (result == 0 && pid > 0).then_some(pid.unsigned_abs())
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::thread::JoinHandle;
    use std::time::Instant;

    use spindle_telemetry::{FRAME_LIMIT, ResourceReading};

    use super::*;

    /// How long a test waits for something the collector does at once.
    const PROMPTLY: Duration = Duration::from_secs(5);

    /// A collector on a socket of its own, stopped and joined when dropped.
    struct Running {
        socket: PathBuf,
        frames: mpsc::Receiver<Collected>,
        counts: Arc<Counts>,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
        _dir: tempfile::TempDir,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    impl Running {
        fn connect(&self) -> UnixStream {
            let stream = UnixStream::connect(&self.socket).expect("the collector listens");
            stream
                .set_read_timeout(Some(PROMPTLY))
                .expect("a read timeout");
            stream
        }

        /// Waits until `done` holds, at most [`PROMPTLY`]; whether it did.
        fn until(&self, done: impl Fn(&Counts) -> bool) -> bool {
            let deadline = Instant::now() + PROMPTLY;
            while !done(&self.counts) {
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            true
        }
    }

    /// A collector with `limits`, its socket named relative to the crate (the working directory of
    /// a test), so the path fits `sun_path` however deep the tree lies. Unit tests have no
    /// `CARGO_TARGET_TMPDIR`; the directory is the same one, `target/tmp`.
    fn running(limits: Limits) -> Running {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let base = manifest.join("../../target/tmp");
        std::fs::create_dir_all(&base).expect("target/tmp");
        let dir = tempfile::tempdir_in(&base).expect("a directory");
        let name = dir.path().file_name().expect("a directory name").to_owned();
        let socket = Path::new("../../target/tmp").join(name).join("c.sock");
        let listener = UnixListener::bind(&socket).expect("the socket binds");
        listener.set_nonblocking(true).expect("non-blocking");
        let (sender, frames) = mpsc::sync_channel(limits.queue);
        let counts = Arc::new(Counts::default());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let counts = Arc::clone(&counts);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || collect(&listener, &sender, limits, &counts, &stop))
        };
        Running {
            socket,
            frames,
            counts,
            stop,
            thread: Some(thread),
            _dir: dir,
        }
    }

    fn limits(deadline: Duration) -> Limits {
        Limits {
            readers: 4,
            queue: 2,
            deadline,
            longest: Duration::from_secs(30),
        }
    }

    /// One frame of `instance` numbered `sequence`, as its line.
    fn line(instance: &str, sequence: u64) -> Vec<u8> {
        let frame = Frame {
            instance_id: instance.to_owned(),
            sequence,
            at: "2026-11-22T00:00:00.000Z".to_owned(),
            period_ms: 1000,
            counters: Vec::new(),
            histograms: Vec::new(),
            recent: Vec::new(),
            resources: ResourceReading {
                cpu_seconds: 0.0,
                memory_bytes: 0,
                cpu_share: None,
                memory_share: None,
            },
            dropped_frames: 0,
        };
        let mut line = serde_json::to_vec(&frame).expect("a frame serialises");
        line.push(b'\n');
        line
    }

    /// Whether the collector closed `stream`: a read sees its end, or a reset.
    fn closed(stream: &mut UnixStream) -> bool {
        let mut byte = [0u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => true,
            Ok(_) => false,
            Err(error) => error.kind() == std::io::ErrorKind::ConnectionReset,
        }
    }

    #[test]
    fn an_oversized_frame_ends_its_connection() {
        let collector = running(limits(Duration::from_secs(30)));
        let mut stream = collector.connect();
        let oversized = vec![b'x'; FRAME_LIMIT + 1];
        let writer = {
            let mut stream = stream.try_clone().expect("a second handle");
            std::thread::spawn(move || {
                let _ = stream.write_all(&oversized);
            })
        };
        let started = Instant::now();
        assert!(
            closed(&mut stream),
            "the collector kept a connection open after {} bytes without a newline (waited {:?})",
            FRAME_LIMIT + 1,
            started.elapsed()
        );
        writer.join().expect("the writer ends");
        assert!(
            collector.until(|counts| counts.readers() == 0),
            "the reader of the oversized connection still runs"
        );
    }

    #[test]
    fn a_silent_connection_is_closed_after_the_deadline() {
        let deadline = Duration::from_millis(300);
        let collector = running(limits(deadline));
        let mut stream = collector.connect();
        let started = Instant::now();
        assert!(
            closed(&mut stream),
            "a connection that sent nothing stayed open for {:?}",
            started.elapsed()
        );
        assert!(
            started.elapsed() >= deadline - Duration::from_millis(50),
            "closed before its deadline: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_connection_dripping_bytes_is_closed_after_the_deadline() {
        let deadline = Duration::from_millis(300);
        let collector = running(limits(deadline));
        let mut stream = collector.connect();
        let writer = {
            let mut stream = stream.try_clone().expect("a second handle");
            std::thread::spawn(move || {
                let started = Instant::now();
                while started.elapsed() < PROMPTLY * 2 {
                    if stream.write_all(b" ").is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
        };
        let started = Instant::now();
        assert!(
            closed(&mut stream),
            "a connection that sent a byte every 50 ms and no frame stayed open for {:?}",
            started.elapsed()
        );
        assert!(
            started.elapsed() < deadline * 4,
            "a connection that sends no frame was held {:?}, past its deadline of {deadline:?}",
            started.elapsed()
        );
        let _ = stream.shutdown(std::net::Shutdown::Both);
        writer.join().expect("the writer ends");
    }

    #[test]
    fn connections_over_the_reader_limit_are_refused() {
        let limits = limits(Duration::from_secs(30));
        let collector = running(limits);
        let mut streams = (0..limits.readers + 10)
            .map(|_| collector.connect())
            .collect::<Vec<_>>();
        for (index, stream) in streams.iter_mut().enumerate().skip(limits.readers) {
            assert!(
                closed(stream),
                "connection {index} over the limit of {} readers was not closed",
                limits.readers
            );
        }
        assert!(
            collector.until(|counts| counts.readers() == limits.readers),
            "{} readers run for {} silent connections within the limit",
            collector.counts.readers(),
            limits.readers
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            collector.counts.readers(),
            limits.readers,
            "more readers than the limit run"
        );
        drop(streams);
        assert!(
            collector.until(|counts| counts.readers() == 0),
            "readers outlive their connections"
        );
    }

    #[test]
    fn a_full_frame_queue_drops_and_counts() {
        let limits = limits(Duration::from_secs(30));
        let collector = running(limits);
        let mut stream = collector.connect();
        for sequence in 0..5 {
            stream
                .write_all(&line("pad-0", sequence))
                .expect("a frame is written");
        }
        assert!(
            collector.until(|counts| counts.dropped() == 3),
            "a queue of 2 nobody takes from dropped {} of 5 frames, not 3",
            collector.counts.dropped()
        );
        stream
            .write_all(&line("pad-0", 5))
            .expect("the reader still reads");
        assert!(
            collector.until(|counts| counts.dropped() == 4),
            "the reader blocked on the full queue: {} dropped",
            collector.counts.dropped()
        );
        let kept = collector
            .frames
            .try_iter()
            .map(|(_, frame)| frame.sequence)
            .collect::<Vec<_>>();
        assert_eq!(kept, [0, 1], "the queue keeps the frames that fit");
        stream
            .write_all(&line("pad-0", 6))
            .expect("a frame is written");
        let (_, next) = collector
            .frames
            .recv_timeout(PROMPTLY)
            .expect("a frame once the queue has room");
        assert_eq!(next.sequence, 6);
    }
}
