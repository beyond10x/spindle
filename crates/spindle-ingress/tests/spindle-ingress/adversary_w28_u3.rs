//! Adversary cases of wave 28, unit U3 (`story:metrics-series-bounded`): `--observe-series
//! aggregated` writes every label whose value comes from the specification empty, and sums the
//! series that then share every label.
//!
//! The runs serve the fixture `tests/fixtures/adversary-w27-u7` (wave 27's): the till's `Post`,
//! reached over the network, publishes `Posted`, which `apply-posted` carries to the balance's
//! `Apply` and `record-posted` to the audit's `Record`; `review-applied` carries `Applied` to the
//! audit's `Review`. Posting 0 is refused by `Record` as `empty` and by `Apply` as `unchanged`,
//! 500 by `Review` as `flagged`, and 5000 by `Record` as `large` while the kernel rejects `Apply`
//! (`broken_invariant`). So one run counts refused bound commands under four outcomes the
//! specification declares.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Retries};
use spindle_ingress::{Listener, SeriesMode, Surface};

const POST: &str = "ledger.till.Post";
const REFUSALS: &str = "spindle_bound_refusals_total";
const ARRIVAL: Duration = Duration::from_secs(10);
/// The refusing outcomes the fixture's specification declares, each reached by one post below.
const DECLARED_REFUSALS: [&str; 4] = ["empty", "unchanged", "flagged", "large"];
/// The amounts posted, one entry each: together they reach every outcome of
/// [`DECLARED_REFUSALS`] and one kernel rejection.
const AMOUNTS: [i64; 4] = [10, 0, 500, 5000];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"observe\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// One `Connection: close` request: the status and the body as text.
fn exchange(
    address: SocketAddr,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> (u16, String) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, answered) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    (status, answered.to_owned())
}

/// The scrape of a served, observing run of the fixture in `mode`, after each of [`AMOUNTS`] was
/// posted once.
fn scrape_after_posts(mode: SeriesMode) -> String {
    let ir = spindle_spec::load(&fixtures().join("adversary-w27-u7"))
        .expect("the fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &["till".to_owned()], None)
        .expect("the till surface is served")
        .with_console(spindle_ingress::HTTP_LOOPBACK)
        .expect("the console is served")
        .observing_series(16, mode);
    let post = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == POST)
        .expect("the surface serves Post");
    let (method, path) = (post.method.to_string().to_uppercase(), post.path.clone());
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::with_retries(
        ir,
        &in_memory(),
        Arc::new(ManualClock::new()),
        state.path(),
        Retries::Retried,
    )
    .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"));
    let listener = Listener::bind_reading(0, surface, host.reader()).expect("the listener binds");
    let address = listener.local_addr();
    for (index, amount) in AMOUNTS.into_iter().enumerate() {
        let (method, path) = (method.clone(), path.clone());
        let body = json!({ "entry_id": format!("entry-{index}"), "amount": amount });
        let asked = thread::spawn(move || exchange(address, &method, &path, Some(&body)));
        listener
            .next(ARRIVAL)
            .expect("the request arrives as an exchange")
            .serve(&mut host);
        let (status, answer) = asked.join().expect("the request thread ends");
        assert!((200..300).contains(&status), "{amount}: {status} {answer}");
    }
    let (status, metrics) = exchange(address, "GET", "/__spindle/metrics", None);
    assert_eq!(status, 200, "{metrics}");
    metrics
}

/// The sample lines of the family `family` (its own name, not a longer one).
fn family_lines<'a>(exposition: &'a str, family: &str) -> Vec<&'a str> {
    exposition
        .lines()
        .filter(|line| {
            line.strip_prefix(family)
                .is_some_and(|rest| rest.starts_with(['{', ' ']))
        })
        .collect()
}

/// story:metrics-series-bounded, Acceptance 1 and "Decided when scoped" D1: aggregated, "every
/// label whose value comes from the specification (`route`, `operation`, `command`, `outcome`,
/// ...) is written empty". `spindle_bound_refusals_total`'s `refusal` label holds the refusing
/// outcome, a name the specification declares, so the aggregated scrape writes one series per
/// declared refusing outcome and grows with the specification's outcomes.
#[test]
fn an_aggregated_scrape_writes_no_refusing_outcome_the_specification_declares() {
    let per_command = scrape_after_posts(SeriesMode::PerCommand);
    let counted = family_lines(&per_command, REFUSALS);
    for outcome in DECLARED_REFUSALS {
        assert!(
            counted
                .iter()
                .any(|line| line.contains(&format!("refusal=\"{outcome}\""))),
            "the run counted a refusal as {outcome}: {counted:#?}"
        );
    }

    let aggregated = scrape_after_posts(SeriesMode::Aggregated);
    let written = family_lines(&aggregated, REFUSALS);
    assert!(
        !written.is_empty(),
        "the refusals are counted:\n{aggregated}"
    );
    let leaked = DECLARED_REFUSALS
        .into_iter()
        .filter(|outcome| {
            written
                .iter()
                .any(|line| line.contains(&format!("refusal=\"{outcome}\"")))
        })
        .collect::<Vec<_>>();
    assert!(
        leaked.is_empty(),
        "the aggregated scrape writes the declared outcomes {leaked:?} as label values:\n{written:#?}"
    );
}

/// The value of every sample line of `exposition` summed per metric name, bucket lines, sums and
/// the mode and scrape-duration families aside.
fn totals(exposition: &str) -> BTreeMap<String, f64> {
    let mut totals = BTreeMap::new();
    for line in exposition.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (series, value) = line.rsplit_once(' ').expect("a sample line has a value");
        let name = series.split('{').next().unwrap_or(series);
        if name.ends_with("_bucket")
            || name.ends_with("_sum")
            || name == "spindle_metrics_series_mode"
            || name == "spindle_metrics_scrape_duration_seconds"
        {
            continue;
        }
        let value: f64 = value
            .parse()
            .unwrap_or_else(|error| panic!("{error}: {line}"));
        *totals.entry(name.to_owned()).or_insert(0.0) += value;
    }
    totals
}

/// Kept probe: aggregating only merges series, it loses no count. The same posts give, family by
/// family, the same total in both modes, and the aggregated scrape writes no more series than the
/// per-command one, and fewer here.
#[test]
fn aggregating_keeps_every_family_total() {
    let per_command = scrape_after_posts(SeriesMode::PerCommand);
    let aggregated = scrape_after_posts(SeriesMode::Aggregated);
    let (kept, summed) = (totals(&per_command), totals(&aggregated));
    assert!(
        kept.contains_key("spindle_http_requests_total")
            && kept.contains_key("spindle_commands_total")
            && kept.contains_key(REFUSALS),
        "{kept:#?}"
    );
    assert_eq!(
        kept, summed,
        "per-command:\n{per_command}\naggregated:\n{aggregated}"
    );
    let lines = |text: &str| {
        text.lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .count()
    };
    assert!(
        lines(&aggregated) < lines(&per_command),
        "per-command:\n{per_command}\naggregated:\n{aggregated}"
    );
    assert!(
        aggregated.contains("spindle_metrics_series_mode{mode=\"aggregated\"} 1"),
        "{aggregated}"
    );
}
