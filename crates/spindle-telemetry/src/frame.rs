//! The `spindle-telemetry/1` frame: one `spindle.observe.Sample`, as one line of JSON.

use std::collections::VecDeque;
use std::io::{self, BufRead};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::counters::{CounterValue, HistogramValue};

/// The name of the wire format: each frame is one JSON object of the shape of
/// `spindle.observe.Sample` (`ess/domains/observe.yaml`), followed by a newline.
pub const PROTOCOL: &str = "spindle-telemetry/1";

/// The most bytes of an event payload a [`RecentAnswer`] keeps.
pub const PAYLOAD_LIMIT: usize = 1024;

/// One sample of one instance: `spindle.observe.Sample`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// `spindle.placement.InstanceId`.
    pub instance_id: String,
    /// Monotonic per instance, starting at 0; a dropped frame uses its number too.
    pub sequence: u64,
    /// When the frame was taken, RFC 3339 in UTC.
    pub at: String,
    /// The sampling period, in milliseconds.
    pub period_ms: u64,
    /// The cumulative counters.
    pub counters: Vec<CounterValue>,
    /// The cumulative histogram buckets.
    pub histograms: Vec<HistogramValue>,
    /// The last command answers, newest first.
    pub recent: Vec<RecentAnswer>,
    /// The instance's own resource readings.
    pub resources: ResourceReading,
    /// Frames dropped since launch because no collector took them.
    pub dropped_frames: u64,
}

/// One command an instance executed: `spindle.observe.RecentAnswer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentAnswer {
    /// When, RFC 3339 in UTC.
    pub at: String,
    /// The command.
    pub command: String,
    /// Its outcome.
    pub outcome: String,
    /// The events it appended.
    pub events: Vec<String>,
    /// The first event's payload as JSON, at most [`PAYLOAD_LIMIT`] bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

/// What an instance reads of its own resources: `spindle.observe.ResourceReading`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceReading {
    /// CPU time used.
    pub cpu_seconds: f64,
    /// Memory in use.
    pub memory_bytes: u64,
    /// CPU time against the request; the instance does not know its request, so it sends none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_share: Option<f64>,
    /// Memory against the request; none, as for `cpu_share`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_share: Option<f64>,
}

/// Reads frames, one per line, from a collector's side of a connection.
#[derive(Debug)]
pub struct FrameReader<R> {
    reader: R,
    line: String,
}

impl<R: BufRead> FrameReader<R> {
    /// Frames from `reader`.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line: String::new(),
        }
    }

    /// The next frame; `None` at the end of the stream.
    pub fn next_frame(&mut self) -> io::Result<Option<Frame>> {
        self.line.clear();
        if self.reader.read_line(&mut self.line)? == 0 {
            return Ok(None);
        }
        serde_json::from_str(&self.line)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}

/// The last answers of an instance, for its frames.
///
/// Recording takes the lock only if nobody holds it: an answer recorded while a frame is being
/// taken is not kept, and the command never waits.
#[derive(Debug)]
pub struct Answers {
    capacity: usize,
    answers: Mutex<VecDeque<RecentAnswer>>,
}

impl Answers {
    /// Keeps the last `capacity` answers.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            answers: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    /// Records one answer now; its payload is cut to [`PAYLOAD_LIMIT`] bytes at a character
    /// boundary. Returns whether it was kept.
    pub fn record(
        &self,
        command: &str,
        outcome: &str,
        events: &[&str],
        payload: Option<&str>,
    ) -> bool {
        if self.capacity == 0 {
            return false;
        }
        let Ok(mut answers) = self.answers.try_lock() else {
            return false;
        };
        if answers.len() == self.capacity {
            answers.pop_back();
        }
        answers.push_front(RecentAnswer {
            at: timestamp(SystemTime::now()),
            command: command.to_owned(),
            outcome: outcome.to_owned(),
            events: events.iter().map(|event| (*event).to_owned()).collect(),
            payload: payload.map(|payload| truncate(payload, PAYLOAD_LIMIT).to_owned()),
        });
        true
    }

    /// The kept answers, newest first.
    pub fn recent(&self) -> Vec<RecentAnswer> {
        self.answers
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

fn truncate(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `at` as RFC 3339 in UTC with milliseconds: `2026-11-04T21:01:02.345Z`.
pub fn timestamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// The proleptic Gregorian date `days` after 1970-01-01.
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_timestamp_is_rfc_3339_in_utc() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        let at = UNIX_EPOCH + Duration::from_millis(1_793_833_262_345);
        assert_eq!(timestamp(at), "2026-11-04T23:01:02.345Z");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(timestamp(leap), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn answers_keep_the_newest_and_cut_payloads() {
        let answers = Answers::new(2);
        let long = "é".repeat(PAYLOAD_LIMIT);
        assert!(answers.record("a.A", "ok", &[], None));
        assert!(answers.record("a.B", "ok", &["a.Bed"], Some(&long)));
        assert!(answers.record("a.C", "ok", &[], None));
        let recent = answers.recent();
        assert_eq!(
            recent
                .iter()
                .map(|answer| answer.command.as_str())
                .collect::<Vec<_>>(),
            ["a.C", "a.B"]
        );
        let payload = recent[1].payload.as_deref().expect("a payload");
        assert!(payload.len() <= PAYLOAD_LIMIT && payload.len() > PAYLOAD_LIMIT - 2);
    }
}
