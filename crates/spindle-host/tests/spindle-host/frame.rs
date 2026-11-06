//! One rally frame, drawn from a `MatchState` view answer.

use serde_json::{Map, Value, json};
use spindle_host::rally::{HEIGHT, WIDTH, frame, winner};

fn view(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn in_play() -> Map<String, Value> {
    view(json!({
        "match_id": "m1",
        "state": "InPlay",
        "ball_x": 3,
        "ball_y": 2,
        "heading_x": "East",
        "heading_y": "Up",
        "west_paddle_y": 5,
        "east_paddle_y": 1,
        "west_score": 1,
        "east_score": 0,
        "target_score": 3,
        "west_needs": 2,
        "east_needs": 3,
    }))
}

/// The grid rows of a frame, each as its cells.
fn grid(frame: &str) -> Vec<Vec<char>> {
    frame
        .lines()
        .filter_map(|line| line.strip_prefix('|')?.strip_suffix('|'))
        .map(|row| row.chars().collect())
        .collect()
}

/// Every cell holding `mark`, as `(x, y)`.
fn cells(grid: &[Vec<char>], mark: char) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (y, row) in grid.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            if *cell == mark {
                found.push((x, y));
            }
        }
    }
    found
}

#[test]
fn the_frame_shows_the_ball_and_both_paddles_where_match_state_says() {
    let drawn = frame(&in_play()).expect("the view draws");
    let rows = grid(&drawn);
    assert_eq!(rows.len(), HEIGHT, "{drawn}");
    assert!(rows.iter().all(|row| row.len() == WIDTH), "{drawn}");
    assert_eq!(cells(&rows, 'o'), [(3, 2)], "{drawn}");
    assert_eq!(cells(&rows, ']'), [(0, 5)], "{drawn}");
    assert_eq!(cells(&rows, '['), [(15, 1)], "{drawn}");
    assert!(drawn.contains("west needs 2"), "{drawn}");
    assert!(drawn.contains("east needs 3"), "{drawn}");
    assert!(!drawn.contains("wins"), "{drawn}");

    let mut moved = in_play();
    moved.insert("ball_x".to_owned(), json!(14));
    moved.insert("ball_y".to_owned(), json!(8));
    moved.insert("west_paddle_y".to_owned(), json!(0));
    moved.insert("east_paddle_y".to_owned(), json!(8));
    let drawn = frame(&moved).expect("the view draws");
    let rows = grid(&drawn);
    assert_eq!(cells(&rows, 'o'), [(14, 8)], "{drawn}");
    assert_eq!(cells(&rows, ']'), [(0, 0)], "{drawn}");
    assert_eq!(cells(&rows, '['), [(15, 8)], "{drawn}");

    let mut won = in_play();
    won.insert("state".to_owned(), json!("Won"));
    won.insert("east_needs".to_owned(), json!(0));
    won.insert("winner".to_owned(), json!("East"));
    let drawn = frame(&won).expect("the view draws");
    assert!(drawn.contains("east needs 0"), "{drawn}");
    assert!(drawn.contains("East wins"), "{drawn}");

    let mut outside = in_play();
    outside.insert("ball_y".to_owned(), json!(9));
    assert!(
        frame(&outside).is_err(),
        "a ball below the grid is not drawn"
    );
    let mut partial = in_play();
    partial.remove("east_paddle_y");
    assert!(
        frame(&partial).is_err(),
        "a view without a paddle is not drawn"
    );
}

#[test]
fn a_winner_is_a_non_empty_side_name_and_the_frame_says_so_only_then() {
    for (value, expected) in [
        (None, None),
        (Some(json!(null)), None),
        (Some(json!("")), None),
        (Some(json!(1)), None),
        (Some(json!("West")), Some("West")),
    ] {
        let mut answer = in_play();
        if let Some(value) = value.clone() {
            answer.insert("winner".to_owned(), value);
        }
        assert_eq!(winner(&answer), expected, "winner {value:?}");
        let drawn = frame(&answer).expect("the view draws");
        assert_eq!(
            drawn.contains("wins"),
            expected.is_some(),
            "winner {value:?}: {drawn}"
        );
    }
}
