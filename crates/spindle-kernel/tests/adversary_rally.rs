//! Adversary cases for `story:rally-rules-are-declared` (commit c4cdf35).
//!
//! Each case drives the kernel from what the rally specification or `docs/design/spindle.md`
//! § 13 says, or replays the unit's own cases in `tests/rally.rs` against a mutated copy of the
//! specification to show which declared rule no case pins down.

mod rally_cases;

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, Executed, MemoryStore, NoExternals, Row, Store, execute};
use spindle_spec::EssIr;

fn spec_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess")
}

fn load(root: &Path) -> EssIr {
    spindle_spec::load(root)
        .unwrap_or_else(|error| panic!("{} compiles: {error:?}", root.display()))
        .ir
}

fn rally() -> EssIr {
    load(&spec_root())
}

fn input(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("an input is an object, not {other}"),
    }
}

fn run(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value) -> Executed {
    execute(ir, store, &mut NoExternals, command, &input(request))
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn showcase() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/spindle.md");
    let text = std::fs::read_to_string(&path).expect("docs/design/spindle.md reads");
    let start = text
        .find("## 13. Showcase: `rally`")
        .expect("§ 13 is the rally showcase");
    let rest = &text[start + 3..];
    let end = rest.find("\n## ").map_or(rest.len(), |end| end + 3);
    text[start..start + end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The same row `tests/rally.rs` builds with `match_row`.
fn match_row(
    state: &str,
    x: i64,
    y: i64,
    heading_x: &str,
    heading_y: &str,
    overrides: Value,
) -> Row {
    let mut fields = json!({
        "ball_x": x,
        "ball_y": y,
        "heading_x": heading_x,
        "heading_y": heading_y,
        "west_paddle_y": 4,
        "east_paddle_y": 4,
        "west_score": 0,
        "east_score": 0,
        "target_score": 3,
        "west_needs": 3,
        "east_needs": 3,
    });
    for (field, value) in overrides.as_object().expect("an object") {
        fields[field] = value.clone();
    }
    Row {
        state: state.to_owned(),
        fields: fields
            .as_object()
            .expect("an object")
            .iter()
            .map(|(field, value)| (field.clone(), value.clone()))
            .collect(),
    }
}

fn seed(store: &mut dyn Store, row: Row) {
    store
        .commit(&Change {
            entity: "rally.table.Match".to_owned(),
            identity: json!("m1"),
            expected: None,
            row,
            events: Vec::new(),
        })
        .expect("the seed commits");
}

fn stored(store: &mut dyn Store, entity: &str) -> Option<Row> {
    store
        .load(entity, &json!("m1"))
        .expect("the store reads")
        .map(|stored| stored.row)
}

/// The heading of the stored match.
fn heading(store: &mut dyn Store) -> (Value, Value) {
    let row = stored(store, "rally.table.Match").expect("the match");
    (
        row.fields["heading_x"].clone(),
        row.fields["heading_y"].clone(),
    )
}

/// § 13: "A serve changes only the state, from `Serving` to `InPlay`". The heading of a serve is
/// driven from that sentence.
#[test]
fn adversary_a_serve_turns_the_ball_as_section_13_states() {
    let section = showcase();
    assert!(
        section.contains("A serve changes only the state, from `Serving` to `InPlay`"),
        "§ 13 no longer makes the claim this case drives"
    );
    let ir = rally();
    let mut store = MemoryStore::default();
    run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    let before = heading(&mut store);
    let served = run(
        &ir,
        &mut store,
        "rally.table.Tick",
        json!({"match_id": "m1"}),
    );
    assert_eq!(served.outcome, "served");
    let after = heading(&mut store);
    assert_eq!(
        before, after,
        "§ 13 says a serve changes only the state; `served` (table.yaml:154) turned {before:?} \
         into {after:?}"
    );
    assert_eq!(
        stored(&mut store, "rally.table.Match")
            .expect("the match")
            .state,
        "InPlay"
    );
}

/// § 13 heads its first rule "A tick moves the ball, serves it, turns it, scores or ends the
/// match", and says a point puts the ball back in the middle.
#[test]
fn adversary_every_tick_moves_or_turns_the_ball() {
    let section = showcase();
    assert!(
        section.contains("A tick moves the ball, serves it, turns it, scores or ends the match")
    );
    assert!(section.contains("A point puts the ball back in the middle"));
    let ir = rally();
    let mut store = MemoryStore::default();
    seed(
        &mut store,
        match_row("InPlay", 1, 2, "West", "Up", json!({"west_paddle_y": 6})),
    );
    let before = stored(&mut store, "rally.table.Match").expect("the match");
    let ticked = run(
        &ir,
        &mut store,
        "rally.table.Tick",
        json!({"match_id": "m1"}),
    );
    let after = stored(&mut store, "rally.table.Match").expect("the match");
    let dx = after.fields["ball_x"].as_i64().unwrap() - before.fields["ball_x"].as_i64().unwrap();
    let dy = after.fields["ball_y"].as_i64().unwrap() - before.fields["ball_y"].as_i64().unwrap();
    let moved = dx.abs() == 1 && dy.abs() == 1;
    let turned = (dx, dy) == (0, 0)
        && (&before.fields["heading_x"], &before.fields["heading_y"])
            != (&after.fields["heading_x"], &after.fields["heading_y"]);
    let scored = (&after.fields["ball_x"], &after.fields["ball_y"]) == (&json!(8), &json!(4))
        && after.state == "Serving";
    assert!(
        moved || turned || scored,
        "`{}` neither moves the ball one cell, turns it nor puts it back in the middle: ({dx}, {dy}), heading {:?} -> {:?}",
        ticked.outcome,
        (&before.fields["heading_x"], &before.fields["heading_y"]),
        (&after.fields["heading_x"], &after.fields["heading_y"]),
    );
}

/// What `tests/rally.rs` asks of the table's invariants, in `the_grid_bounds_are_invariants`,
/// `a_winner_implies_won` and `target_too_low_is_stated`, replayed against `ir`: true when every
/// one of its table cases holds. The cases are the unit's own (`tests/rally_cases`), not a copy.
fn unit_table_cases_hold(ir: &EssIr) -> bool {
    rally_cases::table_cases()
        .iter()
        .all(|case| rally_cases::check(ir, case).is_ok())
}

/// A copy of `examples/rally/ess` with `from` replaced by `to` in `domains/table.yaml`.
fn mutant(directory: &Path, from: &str, to: &str) -> EssIr {
    let root = directory.join("ess");
    std::fs::create_dir_all(root.join("domains")).expect("the copy's directories");
    for file in ["system.yaml", "components.yaml"] {
        std::fs::copy(spec_root().join(file), root.join(file)).expect("the copy");
    }
    for file in ["west.yaml", "east.yaml"] {
        std::fs::copy(
            spec_root().join("domains").join(file),
            root.join("domains").join(file),
        )
        .expect("the copy");
    }
    let table =
        std::fs::read_to_string(spec_root().join("domains/table.yaml")).expect("table.yaml reads");
    assert_eq!(
        table.matches(from).count(),
        1,
        "`{from}` is one line of table.yaml"
    );
    std::fs::write(root.join("domains/table.yaml"), table.replacen(from, to, 1))
        .expect("the mutant writes");
    load(&root)
}

/// Every declared table invariant, dropped or loosened by one, must make some case of
/// `tests/rally.rs` fail: otherwise the suite would stay green with that rule gone.
#[test]
fn adversary_the_unit_cases_kill_every_table_invariant_mutant() {
    assert!(
        unit_table_cases_hold(&rally()),
        "the unit's own cases hold on the unchanged specification"
    );
    let mutants = [
        ("      - ball_x >= 1\n", "      - ball_x >= 0\n"),
        ("      - ball_x <= 14\n", "      - ball_x <= 15\n"),
        ("      - ball_y >= 0\n", ""),
        ("      - ball_y <= 8\n", "      - ball_y <= 9\n"),
        ("      - west_paddle_y >= 0\n", ""),
        ("      - east_paddle_y <= 8\n", ""),
        ("      - target_score >= 1\n", ""),
        ("      - west_needs >= 0\n", ""),
        ("      - east_needs >= 0\n", ""),
        ("      - any: [state != Won, defined(winner)]\n", ""),
        (
            "      - any: [{not: defined(winner)}, {all: [winner == West, west_needs == 0]}, {all: [winner == East, east_needs == 0]}]\n",
            "",
        ),
    ];
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let control = mutant(
        &scratch.path().join("control"),
        "      - west_paddle_y <= 8\n",
        "",
    );
    assert!(
        !unit_table_cases_hold(&control),
        "the replay kills a mutant the unit's cases do catch"
    );
    let mut survivors = Vec::new();
    for (index, (from, to)) in mutants.iter().enumerate() {
        let directory = scratch.path().join(index.to_string());
        let ir = mutant(&directory, from, to);
        if unit_table_cases_hold(&ir) {
            survivors.push(format!("{} -> {:?}", from.trim(), to.trim()));
        }
    }
    assert!(
        survivors.is_empty(),
        "{} of {} table invariant mutants leave every case of tests/rally.rs green:\n{}",
        survivors.len(),
        mutants.len(),
        survivors.join("\n")
    );
}
