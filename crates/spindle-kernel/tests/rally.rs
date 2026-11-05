//! The `rally` example: one match on the table, ticked until a point is scored.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Row, Store, execute,
};
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

// The rules `docs/design/spindle.md` § 13 states for rally, and that the model declares
// (`story:rally-rules-are-declared`). Each test checks the model through the kernel and, where the
// rule is a design statement, that § 13 makes it.

/// `docs/design/spindle.md` § 13, from its heading to the next section or the end, with every run
/// of whitespace one space, so a phrase may wrap.
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

fn states(section: &str, phrase: &str) {
    assert!(
        section.contains(phrase),
        "§ 13 does not state `{phrase}`:\n{section}"
    );
}

/// A match row with the ball at (`x`, `y`) heading (`heading_x`, `heading_y`), both paddles at
/// row 4 unless `overrides` says otherwise.
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

/// Puts `row` in the store as match `m1`, bypassing every command: the arrangement a test needs
/// that no sequence of commands reaches.
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

fn ball(row: &Row) -> (i64, i64, String, String) {
    (
        row.fields["ball_x"].as_i64().expect("an integer"),
        row.fields["ball_y"].as_i64().expect("an integer"),
        row.fields["heading_x"].as_str().expect("a side").to_owned(),
        row.fields["heading_y"]
            .as_str()
            .expect("a vertical")
            .to_owned(),
    )
}

fn tick(ir: &EssIr, store: &mut dyn Store) -> Executed {
    run(ir, store, "rally.table.Tick", json!({"match_id": "m1"}))
}

/// The invariant `command` breaks, or a panic when it executes or fails another way.
fn broken(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value) -> String {
    match execute(ir, store, &mut NoExternals, command, &input(request)) {
        Err(ExecuteError::BrokenInvariant { invariant, .. }) => invariant,
        other => panic!("{command} breaks no invariant: {other:?}"),
    }
}

#[test]
fn each_tick_moves_the_ball_or_the_design_says_when_it_does_not() {
    let section = showcase();
    states(
        &section,
        "a serve, a paddle return and a wall bounce change only the heading",
    );

    let ir = rally();
    let turns = |store: &mut MemoryStore, outcome: &str, row: Option<Row>| {
        if let Some(row) = row {
            seed(store, row);
        }
        let before = ball(&stored(store, "rally.table.Match").expect("the match"));
        let ticked = tick(&ir, store);
        assert_eq!(ticked.outcome, outcome, "{ticked:?}");
        let after = ball(&stored(store, "rally.table.Match").expect("the match"));
        assert_eq!(
            (before.0, before.1),
            (after.0, after.1),
            "{outcome} leaves the ball in place"
        );
        assert_ne!(
            (&before.2, &before.3),
            (&after.2, &after.3),
            "{outcome} turns the ball"
        );
    };

    let mut store = MemoryStore::default();
    run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    let before = ball(&stored(&mut store, "rally.table.Match").expect("the match"));
    let served = tick(&ir, &mut store);
    assert_eq!(served.outcome, "served");
    assert_eq!(
        ball(&stored(&mut store, "rally.table.Match").expect("the match")),
        before,
        "a serve leaves the ball and its heading as the point left them"
    );

    for (outcome, row) in [
        (
            "west-returns",
            match_row("InPlay", 1, 4, "West", "Up", json!({})),
        ),
        (
            "east-returns",
            match_row("InPlay", 14, 4, "East", "Down", json!({})),
        ),
        (
            "top-wall",
            match_row("InPlay", 5, 0, "East", "Up", json!({})),
        ),
        (
            "bottom-wall",
            match_row("InPlay", 5, 8, "West", "Down", json!({})),
        ),
    ] {
        turns(&mut MemoryStore::default(), outcome, Some(row));
    }

    let mut seen = Vec::new();
    for (x, heading_x) in [(5, "East"), (10, "West")] {
        let mut store = MemoryStore::default();
        seed(
            &mut store,
            match_row("InPlay", x, 3, heading_x, "Up", json!({})),
        );
        for _ in 0..32 {
            let before = ball(&stored(&mut store, "rally.table.Match").expect("the match"));
            let ticked = tick(&ir, &mut store);
            let after = ball(&stored(&mut store, "rally.table.Match").expect("the match"));
            if ticked.outcome.starts_with("moved-") {
                assert_eq!(
                    ((after.0 - before.0).abs(), (after.1 - before.1).abs()),
                    (1, 1),
                    "{} moves the ball one cell: {before:?} -> {after:?}",
                    ticked.outcome
                );
                assert_eq!((&before.2, &before.3), (&after.2, &after.3));
            }
            seen.push(ticked.outcome);
        }
    }
    for outcome in [
        "moved-east-up",
        "moved-east-down",
        "moved-west-up",
        "moved-west-down",
    ] {
        assert!(
            seen.iter().any(|seen| seen == outcome),
            "{outcome} in {seen:?}"
        );
    }
}

#[test]
fn value_changes_match_the_design() {
    let section = showcase();
    states(
        &section,
        "point puts the ball back in the middle with literals",
    );
    states(
        &section,
        "a player copies the values its `ObserveBall` input carries",
    );

    let ir = rally();
    let mut store = MemoryStore::default();
    seed(
        &mut store,
        match_row("InPlay", 1, 2, "West", "Up", json!({"west_paddle_y": 6})),
    );
    let scored = tick(&ir, &mut store);
    assert_eq!(scored.outcome, "east-scores", "{scored:?}");
    let row = stored(&mut store, "rally.table.Match").expect("the match");
    assert_eq!(
        (row.fields["ball_x"].clone(), row.fields["ball_y"].clone()),
        (json!(8), json!(4))
    );

    let seen = run(
        &ir,
        &mut store,
        "rally.west.ObserveBall",
        json!({"match_id": "m1", "ball_x": 3, "ball_y": 7, "paddle_y": 2}),
    );
    assert_eq!(seen.outcome, "first-sight");
    let player = stored(&mut store, "rally.west.Player").expect("the player is seated");
    assert_eq!(
        (
            &player.fields["ball_x"],
            &player.fields["ball_y"],
            &player.fields["paddle_y"]
        ),
        (&json!(3), &json!(7), &json!(2))
    );
}

#[test]
fn the_grid_bounds_are_invariants() {
    let ir = rally();

    for (row, command, request, field) in [
        (
            match_row("InPlay", 0, 4, "West", "Up", json!({})),
            "rally.table.Tick",
            json!({"match_id": "m1"}),
            "ball_x",
        ),
        (
            match_row("InPlay", 15, 4, "East", "Up", json!({})),
            "rally.table.Tick",
            json!({"match_id": "m1"}),
            "ball_x",
        ),
        (
            match_row("InPlay", 5, 9, "East", "Down", json!({})),
            "rally.table.Tick",
            json!({"match_id": "m1"}),
            "ball_y",
        ),
        (
            match_row("InPlay", 5, 4, "East", "Up", json!({"west_paddle_y": 9})),
            "rally.table.MovePaddle",
            json!({"match_id": "m1", "side": "West", "step": "Down"}),
            "west_paddle_y",
        ),
        (
            match_row("InPlay", 5, 4, "East", "Up", json!({"east_paddle_y": -1})),
            "rally.table.MovePaddle",
            json!({"match_id": "m1", "side": "East", "step": "Up"}),
            "east_paddle_y",
        ),
    ] {
        let mut store = MemoryStore::default();
        seed(&mut store, row.clone());
        let invariant = broken(&ir, &mut store, command, request);
        assert!(
            invariant.contains(field),
            "{command} breaks `{invariant}`, not one on {field}"
        );
        assert_eq!(
            stored(&mut store, "rally.table.Match"),
            Some(row),
            "nothing changed"
        );
    }

    for side in ["west", "east"] {
        for (ball_x, ball_y, paddle_y, field) in [
            (0, 4, 4, "ball_x"),
            (15, 4, 4, "ball_x"),
            (8, -1, 4, "ball_y"),
            (8, 9, 4, "ball_y"),
            (8, 4, -1, "paddle_y"),
            (8, 4, 9, "paddle_y"),
        ] {
            let mut store = MemoryStore::default();
            let invariant = broken(
                &ir,
                &mut store,
                &format!("rally.{side}.ObserveBall"),
                json!({"match_id": "m1", "ball_x": ball_x, "ball_y": ball_y, "paddle_y": paddle_y}),
            );
            assert!(
                invariant.contains(field),
                "{side} breaks `{invariant}`, not one on {field}"
            );
            assert_eq!(stored(&mut store, &format!("rally.{side}.Player")), None);
        }
    }
}

#[test]
fn a_winner_implies_won() {
    let ir = rally();

    let mut store = MemoryStore::default();
    seed(
        &mut store,
        match_row(
            "InPlay",
            5,
            4,
            "East",
            "Up",
            json!({"winner": "West", "west_needs": 0}),
        ),
    );
    let invariant = broken(
        &ir,
        &mut store,
        "rally.table.MovePaddle",
        json!({"match_id": "m1", "side": "West", "step": "Up"}),
    );
    assert!(invariant.contains("winner"), "{invariant}");

    let mut store = MemoryStore::default();
    seed(
        &mut store,
        match_row(
            "InPlay",
            14,
            4,
            "East",
            "Up",
            json!({"east_paddle_y": 0, "west_needs": 1}),
        ),
    );
    let won = tick(&ir, &mut store);
    assert_eq!(won.outcome, "west-wins", "{won:?}");
    let row = stored(&mut store, "rally.table.Match").expect("the match");
    assert_eq!(
        (row.state.as_str(), &row.fields["winner"]),
        ("Won", &json!("West"))
    );
}

#[test]
fn paddle_ownership_is_decided() {
    let section = showcase();
    states(&section, "The table owns both paddles");
    states(&section, "a copy of its paddle row");

    let ir = rally();
    let mut store = MemoryStore::default();
    run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    for ball_y in [4, 2] {
        run(
            &ir,
            &mut store,
            "rally.west.ObserveBall",
            json!({"match_id": "m1", "ball_x": 8, "ball_y": ball_y, "paddle_y": 4}),
        );
    }
    let player = stored(&mut store, "rally.west.Player").expect("the player is seated");
    assert_eq!(player.fields["paddle_y"], json!(4), "asking moves no copy");
    assert_eq!(
        paddles(&mut store),
        (json!(4), json!(4)),
        "asking moves no paddle"
    );
    run(
        &ir,
        &mut store,
        "rally.table.MovePaddle",
        json!({"match_id": "m1", "side": "West", "step": "Up"}),
    );
    assert_eq!(
        paddles(&mut store),
        (json!(3), json!(4)),
        "the table moves its paddle"
    );
    let player = stored(&mut store, "rally.west.Player").expect("the player is seated");
    assert_eq!(
        player.fields["paddle_y"],
        json!(4),
        "the copy waits for the next ball"
    );
}

#[test]
fn paddle_moved_is_consumed_or_dropped() {
    let section = showcase();
    states(&section, "`PaddleMoved` is published for observers");

    let ir = rally();
    let mut store = MemoryStore::default();
    run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    let moved = run(
        &ir,
        &mut store,
        "rally.table.MovePaddle",
        json!({"match_id": "m1", "side": "East", "step": "Down"}),
    );
    assert_eq!(emitted(&moved)[0]["event"], "rally.table.PaddleMoved");
}

#[test]
fn first_sight_is_stated() {
    let section = showcase();
    states(&section, "`first-sight`");

    let ir = rally();
    for side in ["west", "east"] {
        let mut store = MemoryStore::default();
        let seen = run(
            &ir,
            &mut store,
            &format!("rally.{side}.ObserveBall"),
            json!({"match_id": "m1", "ball_x": 8, "ball_y": 1, "paddle_y": 4}),
        );
        assert_eq!(seen.outcome, "first-sight");
        assert_eq!(
            emitted(&seen),
            vec![json!({
                "event": format!("rally.{side}.BallSeen"),
                "match_id": "m1",
                "ball_x": 8,
                "ball_y": 1,
                "paddle_y": 4,
            })],
            "the first sight asks for nothing, even with the ball above the paddle"
        );
    }
}

#[test]
fn binding_delivery_is_stated() {
    let section = showcase();
    states(&section, "`delivery: at_most_once`");
    states(&section, "`on_failure: drop`");
    states(&section, "`ESS-SYNTH-010`");
}

#[test]
fn target_too_low_is_stated() {
    let section = showcase();
    states(&section, "match to fewer than one point is refused");

    let ir = rally();
    let mut store = MemoryStore::default();
    let refused = run(
        &ir,
        &mut store,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 0}),
    );
    assert_eq!(refused.outcome, "target-too-low");
    assert_eq!(
        refused.error.map(|error| error.name),
        Some("rally.table.TargetScoreTooLow".to_owned())
    );
    assert!(refused.events.is_empty());
    assert_eq!(stored(&mut store, "rally.table.Match"), None);
}
