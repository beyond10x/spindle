//! `GET /__spindle/metrics`: what a served run counted, as the Prometheus text exposition format
//! 0.0.4 ([`METRICS_MEDIA`]), with exactly the families `spindle.observe.MetricFamilies` specifies
//! (`ess/fixtures/observe/metrics/families.json`), in its order.
//!
//! Read when scraped, on the host's thread: the request families from the [`Recorder`], the
//! command, event, binding, timer and store families from the [`Counters`] the host holds per
//! component ([`Host::counters`]), and the rows each entity holds from its store ([`Host::rows`]).
//! A reset empties the stores and leaves every count, so every counter stays monotonic; only
//! `spindle_entity_rows` falls. A scrape counts nothing and is not recorded.
//!
//! A duration histogram has a bucket per bound of [`DURATION_BOUNDS_US`], in seconds, and `+Inf`.
//! The request histogram also has `_sum` and `_count`; a command's has its buckets only, since its
//! counters keep no sum, and its `+Inf` bucket is its count.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use spindle_host::{Host, HostError};
use spindle_telemetry::{Counters, DURATION_BOUNDS_US, Key, Kind};

use super::{Recorder, Traffic};

/// The media type of the metrics: the Prometheus text exposition format, version 0.0.4.
pub const METRICS_MEDIA: &str = "text/plain; version=0.0.4";

/// One metric family, as `spindle.observe.MetricFamily` specifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Family {
    pub(crate) name: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) labels: &'static [&'static str],
    pub(crate) summary: &'static str,
}

/// Every family the metrics expose, in the order `ess/fixtures/observe/metrics/families.json`
/// lists them; a unit test holds the two equal.
pub(crate) const FAMILIES: [Family; 14] = [
    Family {
        name: "spindle_http_requests_total",
        kind: "counter",
        labels: &[
            "method",
            "route",
            "operation",
            "kind",
            "class",
            "status_class",
        ],
        summary: "Requests the run answered outside /__spindle/, by method, route template, \
                  operation, kind, class and status class; an unrouted request has an empty \
                  route, operation and kind.",
    },
    Family {
        name: "spindle_http_request_duration_seconds",
        kind: "histogram",
        labels: &[],
        summary: "From a request's head read to its answer handed back, on the listener thread, \
                  for every request counted by spindle_http_requests_total.",
    },
    Family {
        name: "spindle_commands_total",
        kind: "counter",
        labels: &["command", "outcome"],
        summary: "Commands executed, by the declared outcome the model selected.",
    },
    Family {
        name: "spindle_command_failures_total",
        kind: "counter",
        labels: &["command", "failure"],
        summary: "Commands the kernel did not execute, by why.",
    },
    Family {
        name: "spindle_command_duration_seconds",
        kind: "histogram",
        labels: &["command"],
        summary: "How long the kernel took to execute a command.",
    },
    Family {
        name: "spindle_events_appended_total",
        kind: "counter",
        labels: &["event"],
        summary: "Events appended to a store.",
    },
    Family {
        name: "spindle_bindings_total",
        kind: "counter",
        labels: &["component", "binding", "carried"],
        summary: "Envelopes of an event binding, by what happened to each: sent, delivered, \
                  redelivered or dropped, at the component that measured it.",
    },
    Family {
        name: "spindle_binding_retries_total",
        kind: "counter",
        labels: &["binding", "retried"],
        summary: "Invocations of a retrying binding's command, attempted or exhausted.",
    },
    Family {
        name: "spindle_timer_ticks_total",
        kind: "counter",
        labels: &["binding", "timed"],
        summary: "Ticks of a periodic binding, fired, late or dropped.",
    },
    Family {
        name: "spindle_store_rows_written_total",
        kind: "counter",
        labels: &["component"],
        summary: "Rows a component's store wrote.",
    },
    Family {
        name: "spindle_store_bytes_written_total",
        kind: "counter",
        labels: &["component"],
        summary: "Bytes a component's store wrote.",
    },
    Family {
        name: "spindle_entity_rows",
        kind: "gauge",
        labels: &["entity"],
        summary: "Rows each entity holds now, read from its store when scraped; falls on a reset.",
    },
    Family {
        name: "spindle_resets_total",
        kind: "counter",
        labels: &[],
        summary: "Resets of the run's stores through POST /api/reset.",
    },
    Family {
        name: "spindle_external_causes_total",
        kind: "counter",
        labels: &["command", "outcome", "source"],
        summary: "External causes a served command asked about, by what answered each: world, \
                  file, default or unanswered.",
    },
];

/// One sample's value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    /// A counter's or a gauge's value.
    Number(u64),
    /// A histogram: one count per bucket of [`DURATION_BOUNDS_US`] and one above the last, each
    /// counting only its own bucket; the sum of the values in microseconds, where it is kept.
    Buckets {
        counts: Vec<u64>,
        sum_us: Option<u64>,
    },
}

impl Value {
    /// `self` with `other` added, sample by sample: two counters of one series are one count.
    fn add(&mut self, other: Self) {
        match (self, other) {
            (Self::Number(value), Self::Number(more)) => *value = value.saturating_add(more),
            (
                Self::Buckets { counts, sum_us },
                Self::Buckets {
                    counts: more,
                    sum_us: added,
                },
            ) => {
                for (count, more) in counts.iter_mut().zip(more) {
                    *count = count.saturating_add(more);
                }
                *sum_us = match (*sum_us, added) {
                    (Some(sum), Some(added)) => Some(sum.saturating_add(added)),
                    _ => None,
                };
            }
            (value, other) => *value = other,
        }
    }
}

/// The samples of every family, each series once, in the order first seen.
#[derive(Debug, Default)]
struct Samples {
    families: BTreeMap<&'static str, Vec<(Vec<String>, Value)>>,
}

impl Samples {
    /// Adds `value` to the series of `family` with the label values `labels`.
    fn add(&mut self, family: &'static str, labels: Vec<String>, value: Value) {
        let series = self.families.entry(family).or_default();
        match series.iter_mut().find(|(held, _)| *held == labels) {
            Some((_, held)) => held.add(value),
            None => series.push((labels, value)),
        }
    }

    /// The exposition: each family of [`FAMILIES`] in order, its `# HELP` and `# TYPE`, then its
    /// samples.
    fn text(&self) -> String {
        let mut text = String::new();
        for family in FAMILIES {
            let _ = writeln!(text, "# HELP {} {}", family.name, help(family.summary));
            let _ = writeln!(text, "# TYPE {} {}", family.name, family.kind);
            for (values, value) in self.families.get(family.name).into_iter().flatten() {
                let labels = family
                    .labels
                    .iter()
                    .zip(values)
                    .map(|(label, value)| (*label, value.as_str()))
                    .collect::<Vec<_>>();
                write_sample(&mut text, family.name, &labels, value);
            }
        }
        text
    }
}

/// Writes the lines of one sample of the family `name`.
fn write_sample(text: &mut String, name: &str, labels: &[(&str, &str)], value: &Value) {
    match value {
        Value::Number(value) => {
            let _ = writeln!(text, "{name}{} {value}", series(labels, None));
        }
        Value::Buckets { counts, sum_us } => {
            let mut cumulative = 0u64;
            for (index, count) in counts.iter().enumerate() {
                cumulative = cumulative.saturating_add(*count);
                let bound = DURATION_BOUNDS_US
                    .get(index)
                    .map_or_else(|| "+Inf".to_owned(), |bound| seconds(*bound));
                let _ = writeln!(
                    text,
                    "{name}_bucket{} {cumulative}",
                    series(labels, Some(&bound))
                );
            }
            if let Some(sum_us) = sum_us {
                let _ = writeln!(
                    text,
                    "{name}_sum{} {}",
                    series(labels, None),
                    seconds(*sum_us)
                );
            }
            let _ = writeln!(text, "{name}_count{} {cumulative}", series(labels, None));
        }
    }
}

/// `{label="value",...}`, `le` last where given; empty without any label.
fn series(labels: &[(&str, &str)], le: Option<&str>) -> String {
    let mut pairs = labels
        .iter()
        .map(|(label, value)| format!("{label}=\"{}\"", escaped(value)))
        .collect::<Vec<_>>();
    if let Some(le) = le {
        pairs.push(format!("le=\"{le}\""));
    }
    if pairs.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", pairs.join(","))
    }
}

/// A label value as the text format writes it: `\`, `"` and a line feed escaped.
fn escaped(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('"', "\\\"")
        .replace('\n', r"\n")
}

/// A `# HELP` text as the text format writes it: `\` and a line feed escaped.
fn help(text: &str) -> String {
    text.replace('\\', r"\\").replace('\n', r"\n")
}

/// `micros` microseconds in seconds, written exactly in decimal: `10` is `0.00001`, `1000000` is
/// `1`.
pub(crate) fn seconds(micros: u64) -> String {
    let whole = micros / 1_000_000;
    let fraction = micros % 1_000_000;
    if fraction == 0 {
        return whole.to_string();
    }
    let fraction = format!("{fraction:06}");
    format!("{whole}.{}", fraction.trim_end_matches('0'))
}

/// The metrics of the run `host` runs, the requests `recorder` counted, as the text exposition;
/// the reason they could not be read otherwise, such as a store that cannot be scanned.
pub(crate) fn exposition(recorder: &Recorder, host: &mut Host) -> Result<String, HostError> {
    let mut samples = Samples::default();
    let Traffic {
        requests,
        durations,
        duration_sum_us,
        resets,
    } = recorder.traffic();
    for (labels, count) in requests {
        samples.add(
            "spindle_http_requests_total",
            labels.to_vec(),
            Value::Number(count),
        );
    }
    samples.add(
        "spindle_http_request_duration_seconds",
        Vec::new(),
        Value::Buckets {
            counts: durations,
            sum_us: Some(duration_sum_us),
        },
    );
    let components = host.placements().keys().cloned().collect::<Vec<_>>();
    for component in &components {
        if let Some(counters) = host.counters(component) {
            counted(&mut samples, component, counters);
        }
    }
    for (entity, rows) in host.rows()? {
        samples.add(
            "spindle_entity_rows",
            vec![entity],
            Value::Number(u64::try_from(rows).unwrap_or(u64::MAX)),
        );
    }
    samples.add("spindle_resets_total", Vec::new(), Value::Number(resets));
    Ok(samples.text())
}

/// Adds every counter and histogram of `component`'s `counters` to `samples`, each catalogue
/// entry as one sample of its family.
fn counted(samples: &mut Samples, component: &str, counters: &Counters) {
    let snapshot = counters.snapshot();
    let mut values = snapshot.counters.into_iter();
    let mut histograms = snapshot.histograms.into_iter();
    for counter in &counters.catalogue().counters {
        let value = match counter.kind {
            Kind::Counter => match values.next() {
                Some(value) => Value::Number(value.value),
                None => continue,
            },
            Kind::Histogram => match histograms.next() {
                Some(histogram) => Value::Buckets {
                    counts: histogram.counts,
                    sum_us: Some(histogram.sum_us),
                },
                None => continue,
            },
        };
        let (family, labels): (&'static str, Vec<&str>) = match &counter.key {
            Key::Outcome { command, outcome } => (
                "spindle_commands_total",
                vec![command.as_str(), outcome.as_str()],
            ),
            Key::Failed { command, failure } => (
                "spindle_command_failures_total",
                vec![command.as_str(), failure.as_str()],
            ),
            Key::Duration { command } => {
                ("spindle_command_duration_seconds", vec![command.as_str()])
            }
            Key::Appended { event } => ("spindle_events_appended_total", vec![event.as_str()]),
            Key::Carried { binding, carried } => (
                "spindle_bindings_total",
                vec![component, binding.as_str(), carried.as_str()],
            ),
            Key::Retried { binding, retried } => (
                "spindle_binding_retries_total",
                vec![binding.as_str(), retried.as_str()],
            ),
            Key::Timed { binding, timed } => (
                "spindle_timer_ticks_total",
                vec![binding.as_str(), timed.as_str()],
            ),
            Key::RowsWritten => ("spindle_store_rows_written_total", vec![component]),
            Key::BytesWritten => ("spindle_store_bytes_written_total", vec![component]),
        };
        samples.add(
            family,
            labels.into_iter().map(str::to_owned).collect(),
            value,
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{FAMILIES, seconds};

    /// The families the observe model specifies, read from its fixture.
    fn specified() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ess/fixtures/observe/metrics/families.json"
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("the families are read"))
            .expect("the families are JSON")
    }

    #[test]
    fn the_families_are_the_specified_families() {
        let exposed = FAMILIES
            .iter()
            .map(|family| {
                serde_json::json!({
                    "name": family.name,
                    "kind": family.kind,
                    "labels": family.labels,
                    "summary": family.summary,
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(Value::Array(exposed), specified()["families"]);
    }

    #[test]
    fn a_duration_bound_is_written_in_seconds() {
        assert_eq!(seconds(10), "0.00001");
        assert_eq!(seconds(250), "0.00025");
        assert_eq!(seconds(250_000), "0.25");
        assert_eq!(seconds(1_000_000), "1");
        assert_eq!(seconds(2_500_001), "2.500001");
        assert_eq!(seconds(0), "0");
    }
}
