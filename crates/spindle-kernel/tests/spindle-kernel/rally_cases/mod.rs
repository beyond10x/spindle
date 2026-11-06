//! The rally table's invariant cases, shared by `rally.rs`, which asserts them, and
//! `adversary_rally.rs`, which replays them against mutated copies of the specification.
//!
//! Each case is a match row one step past one declared rule and the invariant that rule is. With a
//! step, the step leaves the field the invariant reads alone, so the row and nothing the step did
//! breaks it. Without one, no command reaches a change from such a row (a won match is preserved,
//! never updated), and the row is checked at rest.

#![allow(dead_code)]

use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, MemoryStore, NoExternals, Row, Store, at_rest, execute,
};
use spindle_spec::EssIr;

/// Which declared rule a case is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// The ball and the paddles stay on the grid.
    Grid,
    /// A winner means a won match, whose side needs no more points.
    Winner,
    /// A match is played to at least one point, and no side needs fewer than none.
    Score,
}

/// One table row past one rule.
#[derive(Debug, Clone)]
pub struct Case {
    /// The rule.
    pub rule: Rule,
    /// The arranged row of match `m1`.
    pub row: Row,
    /// The command and its input, or `None` for a row checked at rest.
    pub step: Option<(&'static str, Value)>,
    /// The invariant that breaks, as the model states it.
    pub invariant: &'static str,
}

/// A match row with the ball at (`x`, `y`) heading (`heading_x`, `heading_y`), both paddles at
/// row 4 and three points to play unless `overrides` says otherwise.
pub fn match_row(
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

/// Puts `row` in the store as match `m1`, bypassing every command: the arrangement a test needs
/// that no sequence of commands reaches.
pub fn seed(store: &mut dyn Store, row: Row) {
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

/// The row of instance `m1` of `entity`, if there is one.
pub fn stored(store: &mut dyn Store, entity: &str) -> Option<Row> {
    store
        .load(entity, &json!("m1"))
        .expect("the store reads")
        .map(|stored| stored.row)
}

fn in_play(overrides: Value) -> Row {
    match_row("InPlay", 5, 4, "East", "Up", overrides)
}

fn moves(side: &str) -> Option<(&'static str, Value)> {
    Some((
        "rally.table.MovePaddle",
        json!({"match_id": "m1", "side": side, "step": "Up"}),
    ))
}

fn tick() -> Option<(&'static str, Value)> {
    Some(("rally.table.Tick", json!({"match_id": "m1"})))
}

/// Every case, one per declared table invariant and side of it, and the ticks that would carry
/// the ball off the grid.
pub fn table_cases() -> Vec<Case> {
    let case = |rule, row, step, invariant| Case {
        rule,
        row,
        step,
        invariant,
    };
    vec![
        case(
            Rule::Grid,
            in_play(json!({"ball_x": 0})),
            moves("West"),
            "ball_x >= 1",
        ),
        case(
            Rule::Grid,
            in_play(json!({"ball_x": 15})),
            moves("West"),
            "ball_x <= 14",
        ),
        case(
            Rule::Grid,
            in_play(json!({"ball_y": -1})),
            moves("West"),
            "ball_y >= 0",
        ),
        case(
            Rule::Grid,
            in_play(json!({"ball_y": 9})),
            moves("West"),
            "ball_y <= 8",
        ),
        case(
            Rule::Grid,
            in_play(json!({"west_paddle_y": -1})),
            moves("East"),
            "west_paddle_y >= 0",
        ),
        case(
            Rule::Grid,
            in_play(json!({"west_paddle_y": 9})),
            moves("East"),
            "west_paddle_y <= 8",
        ),
        case(
            Rule::Grid,
            in_play(json!({"east_paddle_y": -1})),
            moves("West"),
            "east_paddle_y >= 0",
        ),
        case(
            Rule::Grid,
            in_play(json!({"east_paddle_y": 9})),
            moves("West"),
            "east_paddle_y <= 8",
        ),
        case(
            Rule::Grid,
            match_row("InPlay", 0, 4, "West", "Up", json!({})),
            tick(),
            "ball_x >= 1",
        ),
        case(
            Rule::Grid,
            match_row("InPlay", 15, 4, "East", "Up", json!({})),
            tick(),
            "ball_x <= 14",
        ),
        case(
            Rule::Grid,
            match_row("InPlay", 5, 9, "East", "Down", json!({})),
            tick(),
            "ball_y <= 8",
        ),
        case(
            Rule::Score,
            in_play(json!({"target_score": 0, "west_needs": 0, "east_needs": 0})),
            moves("West"),
            "target_score >= 1",
        ),
        case(
            Rule::Score,
            in_play(json!({"west_needs": -1})),
            moves("West"),
            "west_needs >= 0",
        ),
        case(
            Rule::Score,
            in_play(json!({"east_needs": -1})),
            moves("West"),
            "east_needs >= 0",
        ),
        case(
            Rule::Winner,
            in_play(json!({"winner": "West", "west_needs": 0})),
            moves("West"),
            "(state == Won or not (defined(winner)))",
        ),
        case(
            Rule::Winner,
            match_row("Won", 5, 4, "East", "Up", json!({"east_needs": 0})),
            None,
            "(state != Won or defined(winner))",
        ),
        case(
            Rule::Winner,
            match_row(
                "Won",
                5,
                4,
                "East",
                "Up",
                json!({"winner": "West", "west_needs": 1}),
            ),
            None,
            "(not (defined(winner)) or (winner == West and west_needs == 0) or (winner == East and east_needs == 0))",
        ),
    ]
}

fn input(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("an input is an object, not {other}"),
    }
}

/// Whether `case` breaks exactly its invariant on `ir` and leaves the store as arranged; the
/// reason when it does not.
pub fn check(ir: &EssIr, case: &Case) -> Result<(), String> {
    let answer = match &case.step {
        Some((command, request)) => {
            let mut store = MemoryStore::default();
            seed(&mut store, case.row.clone());
            let answer = execute(
                ir,
                &mut store,
                &mut NoExternals,
                command,
                &input(request.clone()),
            );
            if stored(&mut store, "rally.table.Match").as_ref() != Some(&case.row) {
                return Err(format!("{command} changed the arranged row: {answer:?}"));
            }
            answer.map(|executed| format!("{executed:?}"))
        }
        None => at_rest(ir, "rally.table.Match", &json!("m1"), &case.row)
            .map(|()| "the row rests".to_owned()),
    };
    match answer {
        Err(ExecuteError::BrokenInvariant { invariant, .. }) if invariant == case.invariant => {
            Ok(())
        }
        other => Err(format!(
            "{:?} on {:?} breaks not `{}` but: {other:?}",
            case.step, case.row, case.invariant
        )),
    }
}
