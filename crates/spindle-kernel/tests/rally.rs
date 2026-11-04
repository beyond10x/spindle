//! The `rally` example: one match on the table, ticked until a point is scored.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Executed, MemoryStore, NoExternals, Store, execute};
use spindle_spec::EssIr;

fn rally() -> EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess");
    spindle_spec::load(&root)
        .expect("examples/rally/ess compiles")
        .ir
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

#[test]
fn ticking_an_open_match_scores_a_point() {
    let ir = rally();
    let mut store = MemoryStore::default();
    let opened = run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    assert_eq!(opened.outcome, "opened");
    assert_eq!(opened.events[0].name, "rally.table.BallMoved");
    assert_eq!(
        Value::Object(opened.events[0].payload.clone()),
        json!({
            "match_id": "m1",
            "ball_x": 8,
            "ball_y": 4,
            "heading_x": "West",
            "heading_y": "Up",
            "west_paddle_y": 4,
            "east_paddle_y": 4,
        })
    );

    let mut outcomes = Vec::new();
    let mut scored = None;
    for _ in 0..64 {
        let ticked = run(
            &ir,
            &mut store,
            "rally.table.Tick",
            json!({"match_id": "m1"}),
        );
        outcomes.push(ticked.outcome.clone());
        if let Some(point) = ticked
            .events
            .iter()
            .find(|event| event.name == "rally.table.PointScored")
        {
            scored = Some(point.clone());
            break;
        }
    }
    let point = scored.unwrap_or_else(|| panic!("no point in 64 ticks: {outcomes:?}"));
    assert_eq!(point.payload.get("match_id"), Some(&json!("m1")));
    assert_eq!(outcomes.first().map(String::as_str), Some("served"));
    assert!(
        outcomes.iter().any(|outcome| outcome == "top-wall"),
        "{outcomes:?}"
    );

    let side = point.payload.get("side").and_then(Value::as_str);
    let stored = store
        .load("rally.table.Match", &json!("m1"))
        .expect("the store reads")
        .expect("the match exists");
    assert_eq!(
        stored.row.state, "Serving",
        "a point returns the match to serving"
    );
    let (score, needs) = match side {
        Some("East") => ("east_score", "east_needs"),
        Some("West") => ("west_score", "west_needs"),
        other => panic!("a point names a side, not {other:?}"),
    };
    assert_eq!(stored.row.fields.get(score), Some(&json!(1)));
    assert_eq!(stored.row.fields.get(needs), Some(&json!(2)));
    assert_eq!(stored.row.fields.get("ball_x"), Some(&json!(8)));
    assert_eq!(stored.row.fields.get("ball_y"), Some(&json!(4)));
}

#[test]
fn ticking_a_match_nobody_opened_is_the_declared_not_found_error() {
    let ir = rally();
    let mut store = MemoryStore::default();
    let ticked = run(
        &ir,
        &mut store,
        "rally.table.Tick",
        json!({"match_id": "m9"}),
    );
    assert_eq!(ticked.outcome, "no-such-match");
    assert_eq!(
        ticked.error.map(|error| error.name),
        Some("rally.table.MatchNotFound".to_owned())
    );
    assert!(ticked.events.is_empty());
}

#[test]
fn a_player_is_seated_on_first_sight_and_then_follows_the_ball() {
    let ir = rally();
    let mut store = MemoryStore::default();
    let seen = run(
        &ir,
        &mut store,
        "rally.west.ObserveBall",
        json!({"match_id": "m1", "ball_x": 8, "ball_y": 4, "paddle_y": 4}),
    );
    assert_eq!(seen.outcome, "first-sight");
    assert_eq!(seen.events[0].name, "rally.west.BallSeen");

    let up = run(
        &ir,
        &mut store,
        "rally.west.ObserveBall",
        json!({"match_id": "m1", "ball_x": 7, "ball_y": 2, "paddle_y": 4}),
    );
    assert_eq!(up.outcome, "up");
    assert_eq!(
        Value::Object(up.events[0].payload.clone()),
        json!({"match_id": "m1", "side": "West", "step": "Up"})
    );
}

/// The event `executed` emitted, as `{"event": name, ...payload}`, one per event.
fn emitted(executed: &Executed) -> Vec<Value> {
    executed
        .events
        .iter()
        .map(|event| {
            let mut object = event.payload.clone();
            object.insert("event".to_owned(), json!(event.name));
            Value::Object(object)
        })
        .collect()
}

fn paddles(store: &mut dyn Store) -> (Value, Value) {
    let stored = store
        .load("rally.table.Match", &json!("m1"))
        .expect("the store reads")
        .expect("the match exists");
    (
        stored.row.fields["west_paddle_y"].clone(),
        stored.row.fields["east_paddle_y"].clone(),
    )
}

#[test]
fn every_rally_command_executes_on_some_input() {
    let ir = rally();
    let mut store = MemoryStore::default();

    let opened = run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    assert_eq!(opened.outcome, "opened");
    assert_eq!(emitted(&opened)[0]["event"], "rally.table.BallMoved");
    assert_eq!(paddles(&mut store), (json!(4), json!(4)));

    let missing = run(
        &ir,
        &mut store,
        "rally.table.MovePaddle",
        json!({"match_id": "m9", "side": "West", "step": "Up"}),
    );
    assert_eq!(missing.outcome, "no-such-match");
    assert!(missing.events.is_empty());
    assert_eq!(
        missing.error.map(|error| error.name),
        Some("rally.table.MatchNotFound".to_owned())
    );

    for (side, step, outcome, after) in [
        ("West", "Up", "west-up", (3, 4)),
        ("West", "Down", "west-down", (4, 4)),
        ("East", "Up", "east-up", (4, 3)),
        ("East", "Down", "east-down", (4, 4)),
    ] {
        let moved = run(
            &ir,
            &mut store,
            "rally.table.MovePaddle",
            json!({"match_id": "m1", "side": side, "step": step}),
        );
        assert_eq!(moved.outcome, outcome, "{side} {step}");
        assert_eq!(
            emitted(&moved),
            vec![json!({
                "event": "rally.table.PaddleMoved",
                "match_id": "m1",
                "side": side,
                "step": step,
            })]
        );
        assert_eq!(paddles(&mut store), (json!(after.0), json!(after.1)));
    }

    for _ in 0..4 {
        run(
            &ir,
            &mut store,
            "rally.table.MovePaddle",
            json!({"match_id": "m1", "side": "West", "step": "Up"}),
        );
    }
    assert_eq!(paddles(&mut store), (json!(0), json!(4)));
    let at_edge = run(
        &ir,
        &mut store,
        "rally.table.MovePaddle",
        json!({"match_id": "m1", "side": "West", "step": "Up"}),
    );
    assert_eq!(at_edge.outcome, "at-edge");
    assert!(at_edge.events.is_empty());
    assert!(at_edge.error.is_none());
    assert_eq!(paddles(&mut store), (json!(0), json!(4)));

    let served = run(
        &ir,
        &mut store,
        "rally.table.Tick",
        json!({"match_id": "m1"}),
    );
    assert_eq!(served.outcome, "served");
    assert_eq!(emitted(&served)[0]["event"], "rally.table.BallMoved");

    for side in ["west", "east"] {
        let command = format!("rally.{side}.ObserveBall");
        let player = if side == "west" { "West" } else { "East" };
        let seen = format!("rally.{side}.BallSeen");
        let requested = format!("rally.{side}.PaddleMoveRequested");
        let observe = |store: &mut MemoryStore, ball_y: i64| {
            let executed = run(
                &ir,
                store,
                &command,
                json!({"match_id": "m1", "ball_x": 8, "ball_y": ball_y, "paddle_y": 4}),
            );
            (executed.outcome.clone(), emitted(&executed))
        };

        assert_eq!(
            observe(&mut store, 4),
            (
                "first-sight".to_owned(),
                vec![
                    json!({"event": seen, "match_id": "m1", "ball_x": 8, "ball_y": 4, "paddle_y": 4})
                ]
            )
        );
        assert_eq!(
            observe(&mut store, 2),
            (
                "up".to_owned(),
                vec![json!({"event": requested, "match_id": "m1", "side": player, "step": "Up"})]
            )
        );
        assert_eq!(
            observe(&mut store, 6),
            (
                "down".to_owned(),
                vec![json!({"event": requested, "match_id": "m1", "side": player, "step": "Down"})]
            )
        );
        assert_eq!(
            observe(&mut store, 4),
            (
                "hold".to_owned(),
                vec![
                    json!({"event": seen, "match_id": "m1", "ball_x": 8, "ball_y": 4, "paddle_y": 4})
                ]
            )
        );
        let watched = store
            .load(&format!("rally.{side}.Player"), &json!("m1"))
            .expect("the store reads")
            .expect("the player is seated");
        assert_eq!(watched.row.state, "Watching");
        assert_eq!(watched.row.fields.get("ball_y"), Some(&json!(4)));
    }
}
