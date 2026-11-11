//! Adversary cases for `story:view-field-literal-filters`: a view filter comparing a field with a
//! literal, attacked with the scalar types the story's own fixture leaves out (`Timestamp`,
//! `Duration`, `Decimal`, an enum, `Integer` at both ends of its range, a `List<Timestamp>`) and
//! with the `now` operand ESS reads against the clock of the read. Every set of rows is compared
//! with the rows ESS's interpreter answers over the same slips, its clock standing where the
//! host's does.
//!
//! `tests/fixtures/adversary-view-filters` declares one component, `slips`: `IssueSlip` issues a
//! slip with an instant, a wait, an amount, a count, a colour, a list of stamps and an optional
//! note, and each view filters the slips by one comparison.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use ess_conformance::interpret::Interpreted;
use ess_conformance::occurrence_clock::{CommandClock, DecisionInstant};
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

const ISSUE_SLIP: &str = "ledger.slips.IssueSlip";
const OVERDUE: &str = "ledger.slips.Overdue";
const STAMPED_BEFORE: &str = "ledger.slips.StampedBefore";

/// The instant both clocks stand at: 2026-06-01T00:00:00Z.
const NOW: &str = "2026-06-01T00:00:00Z";
const NOW_SECONDS: u64 = 1_780_272_000;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-view-filters")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-view-filters\"\n",
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
    ))
    .expect("the in-memory environment parses")
}

/// The slips every case issues. `s2` is issued at `2026-01-01T00:30:00+01:00`, an instant before
/// `2026-01-01T00:00:00Z` whose spelling sorts after it; it holds no stamp and no note.
fn slips() -> Vec<Value> {
    vec![
        json!({"slip_id": "s1", "issued_at": "2020-01-01T00:00:00Z", "wait": "PT1H",
               "amount": 2.5, "count": 9_223_372_036_854_775_807_i64, "colour": "Red",
               "stamps": ["2020-01-01T00:00:00Z"], "note": "x"}),
        json!({"slip_id": "s2", "issued_at": "2026-01-01T00:30:00+01:00", "wait": "PT60M",
               "amount": 2.49, "count": -9_223_372_036_854_775_808_i64, "colour": "Green",
               "stamps": []}),
        json!({"slip_id": "s3", "issued_at": "2099-01-01T00:00:00Z", "wait": "PT2H",
               "amount": 10, "count": 0, "colour": "Blue",
               "stamps": ["2026-01-01T00:30:00+01:00", "2099-01-01T00:00:00Z"], "note": "y"}),
    ]
}

/// The interpreter's clock, standing at [`NOW`].
struct Fixed;

impl CommandClock for Fixed {
    fn read(&self) -> Option<DecisionInstant> {
        DecisionInstant::parse(NOW).ok()
    }
}

/// A host over memory and ESS's interpreter, both at [`NOW`], with [`slips`] issued on each.
struct Ledger {
    host: Host,
    interpreter: Interpreted,
    views: Vec<String>,
    _state: tempfile::TempDir,
}

impl Ledger {
    fn open() -> Self {
        Self::with(slips())
    }

    /// A ledger with `issued` issued on the host and on the interpreter.
    fn with(issued: Vec<Value>) -> Self {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let ir = spindle_spec::load(&fixture())
            .expect("the adversary fixture compiles")
            .ir;
        let views = ir.views().keys().map(ToString::to_string).collect();
        let interpreter = Interpreted::for_model(ir.clone()).with_command_clock(Fixed);
        let mut host = Host::new(
            ir,
            &in_memory(),
            Arc::new(ManualClock::at(
                UNIX_EPOCH + Duration::from_secs(NOW_SECONDS),
            )),
            state.path(),
        )
        .expect("the host admits the adversary fixture");
        let correlation = CorrelationId::new("adversary-view-filters").expect("a correlation");
        for input in issued {
            let Value::Object(fields) = input.clone() else {
                unreachable!("a slip is an object")
            };
            host.submit(ISSUE_SLIP, fields)
                .unwrap_or_else(|error| panic!("{input} is issued on the host: {error}"));
            let input: BTreeMap<String, Node> =
                serde_json::from_value(input).expect("the input as ESS values");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: ISSUE_SLIP.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{ISSUE_SLIP} runs on the interpreter: {error:?}"));
        }
        Self {
            host,
            interpreter,
            views,
            _state: state,
        }
    }

    /// The slip ids the host answers for `view`, or its refusal.
    fn served(&mut self, view: &str) -> Result<Vec<String>, String> {
        self.host
            .list(view)
            .map(|rows| ids(rows.iter().map(|row| row.get("slip_id").cloned())))
            .map_err(|error| error.to_string())
    }

    /// The slip ids ESS's interpreter answers for `view`, or `None` where it refuses the read.
    fn interpreted(&self, view: &str) -> Option<Vec<String>> {
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params: BTreeMap::new(),
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("adversary-view-filters").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .ok()?;
        Some(ids(answer.rows.iter().map(|row| {
            row.get("slip_id")
                .map(|id| serde_json::to_value(id).expect("an id as JSON"))
        })))
    }
}

fn ids(rows: impl Iterator<Item = Option<Value>>) -> Vec<String> {
    let mut ids = rows
        .map(|id| {
            id.and_then(|id| id.as_str().map(str::to_owned))
                .expect("a row with a slip id")
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

/// A filter ordering a `Timestamp` field against `now` is admitted by the host (its literal is
/// text, and a `Timestamp` is ordered), so the route claims to serve it; ESS's interpreter, given
/// the clock of the read, answers the slips issued before it. The host either refuses such a view
/// whole before reading (as it refuses every filter it does not read) or answers those rows: it
/// may not admit the view and then refuse every read that finds a row as "unknown".
#[test]
fn a_filter_ordering_a_field_against_now_is_answered_or_refused_whole() {
    let mut ledger = Ledger::open();
    assert_eq!(
        ledger.interpreted(OVERDUE),
        Some(vec!["s1".to_owned(), "s2".to_owned()]),
        "ESS's interpreter, its clock at {NOW}, answers the slips issued before it"
    );
    for view in [OVERDUE, STAMPED_BEFORE] {
        let admitted = spindle_host::reads(ledger.host.ir(), view);
        let served = ledger.served(view);
        assert!(
            admitted.is_err() || served.is_ok(),
            "{view} is admitted ({admitted:?}) and then refused on a read: {served:?}"
        );
    }
}

/// Every view of the fixture answers the rows ESS's interpreter answers with its clock where the
/// host's stands, or is refused where the interpreter refuses the read.
#[test]
fn literal_filters_over_every_scalar_agree_with_the_interpreter() {
    let mut ledger = Ledger::open();
    let mut differences = Vec::new();
    for view in ledger.views.clone() {
        let served = ledger.served(&view).ok();
        let interpreted = ledger.interpreted(&view);
        if served != interpreted {
            differences.push(format!(
                "{view}: host {served:?}, interpreter {interpreted:?}"
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "views whose rows differ from ESS's interpreter: {differences:#?}"
    );
}

/// A slip issued at `issued_at`, every other field as `s1` holds it.
fn slip(id: &str, issued_at: &str) -> Value {
    json!({"slip_id": id, "issued_at": issued_at, "wait": "PT1H", "amount": 1, "count": 1,
           "colour": "Red", "stamps": []})
}

/// `story:view-filter-reads-now`: a filter `issued_at < now - 5m` (`ess/34` admits `now` in a view
/// filter from `ess/30`) is read, so the route answers without `501`, and keeps the slips issued
/// more than five minutes before the clock of the read: `old`, ten minutes before, and `offset`,
/// six minutes before in a `+01:00` spelling; not `edge`, issued exactly five minutes before, nor
/// `young`, one minute before. The host's thread, its reader and ESS's interpreter, its clock
/// where the host's stands, answer the same slips.
#[test]
fn a_filter_ordering_against_now_minus_five_minutes_keeps_the_older_rows() {
    const STALE: &str = "ledger.slips.Stale";
    let mut ledger = Ledger::with(vec![
        slip("old", "2026-05-31T23:50:00Z"),
        slip("offset", "2026-06-01T00:54:00+01:00"),
        slip("edge", "2026-05-31T23:55:00Z"),
        slip("young", "2026-05-31T23:59:00Z"),
    ]);
    assert!(
        spindle_host::reads(ledger.host.ir(), STALE).is_ok(),
        "{STALE} is read: {:?}",
        spindle_host::reads(ledger.host.ir(), STALE)
    );
    let older = vec!["offset".to_owned(), "old".to_owned()];
    assert_eq!(ledger.served(STALE), Ok(older.clone()), "the host's thread");
    let beside = ledger
        .host
        .reader()
        .list(STALE)
        .map(|rows| ids(rows.iter().map(|row| row.get("slip_id").cloned())))
        .map_err(|error| error.to_string());
    assert_eq!(beside, Ok(older.clone()), "the host's reader");
    assert_eq!(
        ledger.interpreted(STALE),
        Some(older),
        "ESS's interpreter, its clock at {NOW}"
    );
}
