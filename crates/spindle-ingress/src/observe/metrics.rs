//! `GET /__spindle/metrics`: what a served run counted, as the Prometheus text exposition format
//! 0.0.4 ([`METRICS_MEDIA`]), with exactly the families `spindle.observe.MetricFamilies` specifies
//! (`ess/fixtures/observe/metrics/families.json`), in its order.
//!
//! Read when scraped, on the host's thread: the request and external cause families from the
//! [`Recorder`] (the external causes as the run's decider noted them, [`Recorder::answered`]), the
//! command, event, binding, timer and store families from the [`Counters`] the host holds per
//! component ([`Host::counters`]), and the rows each entity holds as the host keeps them from its
//! store's writes ([`Host::rows`]), so a scrape reads no store and holds the host's thread only
//! while it writes the series.
//! A reset empties the stores and leaves every count, so every counter stays monotonic; only
//! `spindle_entity_rows` falls. A scrape counts nothing and is not recorded.
//!
//! A duration histogram has a bucket per bound of [`DURATION_BOUNDS_US`], in seconds, and `+Inf`.
//! The request histogram also has `_sum` and `_count`; a command's has its buckets only, since its
//! counters keep no sum, and its `+Inf` bucket is its count.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
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
                  file, default, call or unanswered.",
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

/// The series of one family, each once, in the order first seen, each under its labels as the
/// text writes them ([`pairs`]) and found by them through an index, not a search of every
/// series. The escaping is one-to-one, so two series share the text only where they share every
/// label value.
#[derive(Debug, Default)]
struct Series {
    held: Vec<(String, Value)>,
    /// Where in `held` the series with each label text is.
    at: HashMap<String, usize>,
}

/// The samples of every family, each series once, in the order first seen.
#[derive(Debug, Default)]
struct Samples {
    families: BTreeMap<&'static str, Series>,
}

impl Samples {
    /// Adds `value` to the series of `family` with the label values `values`, one per label of the
    /// family in its order.
    fn add(&mut self, family: &'static str, values: &[impl AsRef<str>], value: Value) {
        let names = FAMILIES
            .iter()
            .find(|declared| declared.name == family)
            .map_or(&[][..], |declared| declared.labels);
        let labels = pairs(names, values);
        let series = self.families.entry(family).or_default();
        let next = series.held.len();
        match series.at.entry(labels) {
            Entry::Occupied(at) => series.held[*at.get()].1.add(value),
            Entry::Vacant(vacant) => {
                series.held.push((vacant.key().clone(), value));
                vacant.insert(next);
            }
        }
    }

    /// The exposition: each family of [`FAMILIES`] in order, its `# HELP` and `# TYPE`, then its
    /// samples, each bucket bound written once.
    fn text(&self) -> String {
        let bounds = DURATION_BOUNDS_US
            .iter()
            .map(|bound| seconds(*bound))
            .collect::<Vec<_>>();
        let mut text = String::new();
        for family in FAMILIES {
            let _ = writeln!(text, "# HELP {} {}", family.name, help(family.summary));
            let _ = writeln!(text, "# TYPE {} {}", family.name, family.kind);
            let held = self.families.get(family.name).map(|series| &series.held);
            for (labels, value) in held.into_iter().flatten() {
                write_sample(&mut text, family.name, labels, value, &bounds);
            }
        }
        text
    }
}

/// Writes the lines of one sample of the family `name`: `labels` its label pairs as
/// [`pairs`] writes them, `bounds` each bucket's bound in seconds, the last bucket's `+Inf`.
fn write_sample(text: &mut String, name: &str, labels: &str, value: &Value, bounds: &[String]) {
    match value {
        Value::Number(value) => {
            text.push_str(name);
            braced(text, labels, None);
            let _ = writeln!(text, " {value}");
        }
        Value::Buckets { counts, sum_us } => {
            let mut cumulative = 0u64;
            for (index, count) in counts.iter().enumerate() {
                cumulative = cumulative.saturating_add(*count);
                let bound = bounds.get(index).map_or("+Inf", String::as_str);
                text.push_str(name);
                text.push_str("_bucket");
                braced(text, labels, Some(bound));
                let _ = writeln!(text, " {cumulative}");
            }
            if let Some(sum_us) = sum_us {
                text.push_str(name);
                text.push_str("_sum");
                braced(text, labels, None);
                let _ = writeln!(text, " {}", seconds(*sum_us));
            }
            text.push_str(name);
            text.push_str("_count");
            braced(text, labels, None);
            let _ = writeln!(text, " {cumulative}");
        }
    }
}

/// `label="value",...`, each label of `labels` with its value of `values`, escaped
/// ([`escape`]); empty without any label.
fn pairs(labels: &[&str], values: &[impl AsRef<str>]) -> String {
    let mut text = String::with_capacity(64);
    for (index, (label, value)) in labels.iter().zip(values).enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(label);
        text.push_str("=\"");
        escape(&mut text, value.as_ref());
        text.push('"');
    }
    text
}

/// Writes `{<labels>}`, with `le` last where given; nothing without a label or `le`.
fn braced(text: &mut String, labels: &str, le: Option<&str>) {
    if labels.is_empty() && le.is_none() {
        return;
    }
    text.push('{');
    text.push_str(labels);
    if let Some(le) = le {
        if !labels.is_empty() {
            text.push(',');
        }
        text.push_str("le=\"");
        text.push_str(le);
        text.push('"');
    }
    text.push('}');
}

/// Writes a label value as the text format writes it: `\`, `"` and a line feed escaped.
fn escape(text: &mut String, value: &str) {
    if !(value.contains('\\') || value.contains('"') || value.contains('\n')) {
        text.push_str(value);
        return;
    }
    for character in value.chars() {
        match character {
            '\\' => text.push_str(r"\\"),
            '"' => text.push_str("\\\""),
            '\n' => text.push_str(r"\n"),
            other => text.push(other),
        }
    }
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
/// the reason they could not be read otherwise: a store whose rows had to be counted and could not
/// be read, as one a failed reset lost ([`Host::rows`]).
pub(crate) fn exposition(recorder: &Recorder, host: &mut Host) -> Result<String, HostError> {
    let mut samples = Samples::default();
    let Traffic {
        requests,
        durations,
        duration_sum_us,
        resets,
        externals,
    } = recorder.traffic();
    for (labels, count) in requests {
        samples.add("spindle_http_requests_total", &labels, Value::Number(count));
    }
    samples.add(
        "spindle_http_request_duration_seconds",
        &[] as &[&str],
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
            &[entity],
            Value::Number(u64::try_from(rows).unwrap_or(u64::MAX)),
        );
    }
    samples.add(
        "spindle_resets_total",
        &[] as &[&str],
        Value::Number(resets),
    );
    for (labels, count) in externals {
        samples.add(
            "spindle_external_causes_total",
            &labels,
            Value::Number(count),
        );
    }
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
        samples.add(family, &labels, value);
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

    /// The samples and their text as 0.11.0 wrote them, word for word: each series found by a
    /// search of every series of its family, and each line's labels escaped again
    /// (`story:metrics-read-without-scan`).
    mod reference {
        use std::fmt::Write as _;

        use super::super::{DURATION_BOUNDS_US, FAMILIES, Value, help, seconds};

        #[derive(Default)]
        pub(super) struct Samples {
            families: std::collections::BTreeMap<&'static str, Vec<(Vec<String>, Value)>>,
        }

        impl Samples {
            pub(super) fn add(&mut self, family: &'static str, labels: Vec<String>, value: Value) {
                let series = self.families.entry(family).or_default();
                match series.iter_mut().find(|(held, _)| *held == labels) {
                    Some((_, held)) => held.add(value),
                    None => series.push((labels, value)),
                }
            }

            pub(super) fn text(&self) -> String {
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

        fn escaped(value: &str) -> String {
            value
                .replace('\\', r"\\")
                .replace('"', "\\\"")
                .replace('\n', r"\n")
        }
    }

    #[test]
    fn the_exposition_is_written_byte_for_byte_as_before() {
        use super::{Samples, Value as Sample};

        let buckets = |offset: u64, sum_us: Option<u64>| Sample::Buckets {
            counts: (0..=12).map(|index| (index * 7 + offset) % 5).collect(),
            sum_us,
        };
        let labelled = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
        let added: Vec<(&'static str, Vec<String>, Sample)> = vec![
            (
                "spindle_http_requests_total",
                labelled(&["GET", "/a/{b}", "x.y.Z", "view", "served", "2xx"]),
                Sample::Number(3),
            ),
            (
                "spindle_http_request_duration_seconds",
                Vec::new(),
                buckets(1, Some(2_500_001)),
            ),
            (
                "spindle_commands_total",
                labelled(&["x.y.Open", "opened"]),
                Sample::Number(1),
            ),
            (
                "spindle_commands_total",
                labelled(&["x.y.Take", "a \"quoted\" \\ back\nline"]),
                Sample::Number(4),
            ),
            (
                "spindle_commands_total",
                labelled(&["x.y.Open", "opened"]),
                Sample::Number(6),
            ),
            (
                "spindle_command_duration_seconds",
                labelled(&["x.y.Open"]),
                buckets(2, None),
            ),
            (
                "spindle_command_duration_seconds",
                labelled(&["x.y.Take"]),
                buckets(3, Some(10)),
            ),
            (
                "spindle_command_duration_seconds",
                labelled(&["x.y.Open"]),
                buckets(4, None),
            ),
            (
                "spindle_entity_rows",
                labelled(&["x.y.Desk"]),
                Sample::Number(0),
            ),
            ("spindle_resets_total", Vec::new(), Sample::Number(2)),
            ("spindle_resets_total", Vec::new(), Sample::Number(5)),
        ];
        let mut samples = Samples::default();
        let mut before = reference::Samples::default();
        for (family, labels, value) in added {
            samples.add(family, &labels, value.clone());
            before.add(family, labels, value);
        }
        let text = samples.text();
        assert_eq!(text, before.text());
        assert!(
            text.contains(r#"spindle_commands_total{command="x.y.Open",outcome="opened"} 7"#),
            "{text}"
        );
        assert!(
            text.contains(r#"outcome="a \"quoted\" \\ back\nline"} 4"#),
            "{text}"
        );
    }
}
