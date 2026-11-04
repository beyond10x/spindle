//! The sender: one frame per sample period from an instance's counters to its endpoint.

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crate::counters::Counters;
use crate::endpoint::{
    Connection, DEFAULT_PERIOD, ENV_TELEMETRY, ENV_TELEMETRY_PERIOD_MS, Endpoint,
};
use crate::frame::{Answers, Frame, ResourceReading, timestamp};
use crate::resources::{Readings, read_resources};

/// The instance id variable a launcher sets.
const ENV_INSTANCE_ID: &str = "SPINDLE_INSTANCE_ID";

/// How an instance sends its frames.
#[derive(Debug, Clone)]
pub struct SenderOptions {
    /// The instance id each frame carries.
    pub instance_id: String,
    /// Where frames go.
    pub endpoint: Endpoint,
    /// The sample period; also how long a write may wait before the frame is dropped.
    pub period: Duration,
    /// Whose resources each frame reports.
    pub readings: Readings,
    /// The answers each frame carries; none without.
    pub answers: Option<Arc<Answers>>,
}

impl SenderOptions {
    /// Options for `instance_id` sending to `endpoint` every `period`, reading the process's
    /// resources for a Unix endpoint and the machine's for vsock, carrying no answers.
    pub fn new(instance_id: impl Into<String>, endpoint: Endpoint, period: Duration) -> Self {
        let readings = match endpoint {
            Endpoint::Unix(_) => Readings::Process,
            Endpoint::Vsock { .. } => Readings::Machine,
        };
        Self {
            instance_id: instance_id.into(),
            endpoint,
            period: period.max(Duration::from_millis(1)),
            readings,
            answers: None,
        }
    }

    /// The same options reading `readings`.
    pub fn with_readings(mut self, readings: Readings) -> Self {
        self.readings = readings;
        self
    }

    /// The same options carrying `answers`.
    pub fn with_answers(mut self, answers: Arc<Answers>) -> Self {
        self.answers = Some(answers);
        self
    }

    /// The options a launcher gave this process: `SPINDLE_TELEMETRY`, `SPINDLE_INSTANCE_ID` and,
    /// if set, `SPINDLE_TELEMETRY_PERIOD_MS`. `Ok(None)` when no endpoint was given; a variable
    /// that is set but not UTF-8 is an error, never an absent endpoint.
    pub fn from_env() -> Result<Option<Self>, String> {
        for name in [ENV_TELEMETRY, ENV_INSTANCE_ID, ENV_TELEMETRY_PERIOD_MS] {
            if let Err(std::env::VarError::NotUnicode(value)) = std::env::var(name) {
                return Err(format!("{name}={} is not UTF-8", value.display()));
            }
        }
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// As [`Self::from_env`], reading each variable through `lookup`.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let Some(endpoint) = lookup(ENV_TELEMETRY) else {
            return Ok(None);
        };
        let endpoint: Endpoint = endpoint
            .parse()
            .map_err(|error| format!("{ENV_TELEMETRY}: {error}"))?;
        let instance_id = lookup(ENV_INSTANCE_ID)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| format!("{ENV_TELEMETRY} is set but {ENV_INSTANCE_ID} is not"))?;
        let period = match lookup(ENV_TELEMETRY_PERIOD_MS) {
            None => DEFAULT_PERIOD,
            Some(millis) => match millis.parse::<u64>() {
                Ok(millis) if millis > 0 => Duration::from_millis(millis),
                _ => {
                    return Err(format!(
                        "{ENV_TELEMETRY_PERIOD_MS}={millis:?} is not a positive number of \
                         milliseconds"
                    ));
                }
            },
        };
        Ok(Some(Self::new(instance_id, endpoint, period)))
    }
}

/// What became of one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Written whole to the collector's socket. On a Unix socket that is not proof a collector
    /// read it, or even accepted the connection: the kernel queues a connection a listener has
    /// not accepted and takes writes into it until its buffer is full. A write that times out
    /// is [`Self::Dropped`].
    Sent,
    /// Not delivered: no collector, or one too slow or gone. Counted in the next frames.
    Dropped,
}

/// Takes and sends frames one at a time, on the caller's thread. [`Sender`] calls it once a
/// period; a test calls it directly.
///
/// A frame's counters are a [`Counters::snapshot`], so a lost frame loses no count: the next
/// carries every total again. A frame is dropped, never queued, when it cannot be written: the
/// connection is closed, and the next frame connects anew.
#[derive(Debug)]
pub struct Transmitter {
    counters: Arc<Counters>,
    options: SenderOptions,
    sequence: u64,
    dropped: u64,
    sent: u64,
    connection: Option<Connection>,
}

impl Transmitter {
    /// A transmitter of `counters`, connecting at its first frame.
    pub fn new(counters: Arc<Counters>, options: SenderOptions) -> Self {
        Self {
            counters,
            options,
            sequence: 0,
            dropped: 0,
            sent: 0,
            connection: None,
        }
    }

    /// The frame that would be sent now, numbered with the next sequence number.
    pub fn frame(&self) -> Frame {
        let snapshot = self.counters.snapshot();
        Frame {
            instance_id: self.options.instance_id.clone(),
            sequence: self.sequence,
            at: timestamp(SystemTime::now()),
            period_ms: u64::try_from(self.options.period.as_millis())
                .unwrap_or(u64::MAX)
                .max(1),
            counters: snapshot.counters,
            histograms: snapshot.histograms,
            recent: self
                .options
                .answers
                .as_ref()
                .map(|answers| answers.recent())
                .unwrap_or_default(),
            resources: read_resources(self.options.readings).unwrap_or(ResourceReading {
                cpu_seconds: 0.0,
                memory_bytes: 0,
                cpu_share: None,
                memory_share: None,
            }),
            dropped_frames: self.dropped,
        }
    }

    /// Takes the next frame and sends it.
    pub fn send(&mut self) -> Delivery {
        let frame = self.frame();
        self.sequence += 1;
        let mut line = match serde_json::to_vec(&frame) {
            Ok(line) => line,
            Err(_) => return self.drop_frame(),
        };
        line.push(b'\n');
        if self.connection.is_none() {
            match self.options.endpoint.connect(self.options.period) {
                Ok(connection) => self.connection = Some(connection),
                Err(_) => return self.drop_frame(),
            }
        }
        let period = self.options.period;
        let written = self
            .connection
            .as_mut()
            .map_or(Err(io::ErrorKind::NotConnected.into()), |connection| {
                write_within(connection, &line, period)
            });
        match written {
            Ok(()) => {
                self.sent += 1;
                Delivery::Sent
            }
            Err(_) => self.drop_frame(),
        }
    }

    /// Frames written whole to the socket; see [`Delivery::Sent`].
    pub fn sent(&self) -> u64 {
        self.sent
    }

    /// Frames dropped.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    fn drop_frame(&mut self) -> Delivery {
        self.connection = None;
        self.dropped += 1;
        Delivery::Dropped
    }
}

/// Writes all of `bytes`, giving up once `within` has passed: each write waits at most what is
/// left of it, so a collector that stops reading holds the sender one period at most.
fn write_within(connection: &mut Connection, bytes: &[u8], within: Duration) -> io::Result<()> {
    let deadline = Instant::now() + within;
    let mut written = 0;
    while written < bytes.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        connection.set_write_timeout(left.max(Duration::from_micros(1)))?;
        match connection.write(&bytes[written..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Shared {
    stop: AtomicBool,
    sent: AtomicU64,
    dropped: AtomicU64,
}

/// Sends a frame of an instance's counters every period, on a thread of its own.
///
/// Recording never waits for it: the runtime increments atomics, and this thread reads them. With
/// no collector, or a slow one, frames are dropped and counted ([`Frame::dropped_frames`]).
/// Dropping the sender stops it.
#[derive(Debug)]
pub struct Sender {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Sender {
    /// Starts sending `counters` as `options` say; the first frame goes out at once.
    ///
    /// An endpoint [`Endpoint::check`] refuses is refused here, by name.
    pub fn start(counters: Arc<Counters>, options: SenderOptions) -> io::Result<Self> {
        options
            .endpoint
            .check()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let shared = Arc::new(Shared::default());
        let period = options.period;
        let mut transmitter = Transmitter::new(counters, options);
        let state = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("spindle-telemetry".to_owned())
            .spawn(move || {
                let mut next = Instant::now();
                while !state.stop.load(Ordering::Relaxed) {
                    transmitter.send();
                    state.sent.store(transmitter.sent(), Ordering::Relaxed);
                    state
                        .dropped
                        .store(transmitter.dropped(), Ordering::Relaxed);
                    next += period;
                    let now = Instant::now();
                    if next < now {
                        next = now;
                    }
                    while !state.stop.load(Ordering::Relaxed) {
                        let left = next.saturating_duration_since(Instant::now());
                        if left.is_zero() {
                            break;
                        }
                        std::thread::park_timeout(left);
                    }
                }
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    /// Starts the sender the launcher asked for in the environment ([`SenderOptions::from_env`]);
    /// `Ok(None)` when it asked for none.
    pub fn from_env(counters: Arc<Counters>) -> Result<Option<Self>, String> {
        match SenderOptions::from_env()? {
            None => Ok(None),
            Some(options) => Self::start(counters, options)
                .map(Some)
                .map_err(|error| format!("starting the telemetry sender: {error}")),
        }
    }

    /// Frames written whole to the socket so far; see [`Delivery::Sent`].
    pub fn sent(&self) -> u64 {
        self.shared.sent.load(Ordering::Relaxed)
    }

    /// Frames dropped so far.
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Waits until `frames` frames were sent, at most `within`; whether they were.
    pub fn wait_until_sent(&self, frames: u64, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while self.sent() < frames {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    /// Stops sending and waits for the thread: at most one period for a connect and one write
    /// timeout (also one period) for the frame it is writing.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.halt();
    }
}
