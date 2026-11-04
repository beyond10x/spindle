//! The terminal frame of the `rally` example, drawn from the table's `MatchState` view answer.
//!
//! The frame reads the view answer only, never a stored row: what a person watches is what the
//! specification promises the outside world can observe.

use serde_json::{Map, Value};

/// Columns of the grid: x from 0 to 15.
pub const WIDTH: usize = 16;
/// Rows of the grid: y from 0 at the top to 8.
pub const HEIGHT: usize = 9;

/// The ball.
pub const BALL: char = 'o';
/// The west paddle, in column 0.
pub const WEST_PADDLE: char = ']';
/// The east paddle, in column 15.
pub const EAST_PADDLE: char = '[';

/// Why a view answer cannot be drawn.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// The answer has no integer under a field the frame draws.
    #[error("the view answers no integer `{0}`")]
    Missing(&'static str),
    /// A position lies outside the grid.
    #[error("`{field}` is {value}, outside the grid")]
    OutsideGrid {
        /// The field.
        field: &'static str,
        /// Its value.
        value: i64,
    },
}

/// The side that won, read from a `MatchState` view answer: a non-empty text under `winner`.
///
/// The one place that decides whether a match has a winner; an absent, `null`, empty or
/// non-text `winner` is none.
pub fn winner(view: &Map<String, Value>) -> Option<&str> {
    view.get("winner")
        .and_then(Value::as_str)
        .filter(|side| !side.is_empty())
}

/// Draws one frame: the points each side still needs, the grid with the ball and both paddles,
/// and the winner once there is one.
///
/// # Errors
/// [`FrameError`] when `view` lacks a field the frame draws or places something off the grid.
pub fn frame(view: &Map<String, Value>) -> Result<String, FrameError> {
    let ball_x = position(view, "ball_x", WIDTH)?;
    let ball_y = position(view, "ball_y", HEIGHT)?;
    let west = position(view, "west_paddle_y", HEIGHT)?;
    let east = position(view, "east_paddle_y", HEIGHT)?;
    let west_needs = integer(view, "west_needs")?;
    let east_needs = integer(view, "east_needs")?;

    let mut out = format!("west needs {west_needs}    east needs {east_needs}\n");
    let border = format!("+{}+\n", "-".repeat(WIDTH));
    out.push_str(&border);
    for y in 0..HEIGHT {
        out.push('|');
        for x in 0..WIDTH {
            out.push(if (x, y) == (ball_x, ball_y) {
                BALL
            } else if x == 0 && y == west {
                WEST_PADDLE
            } else if x == WIDTH - 1 && y == east {
                EAST_PADDLE
            } else {
                ' '
            });
        }
        out.push_str("|\n");
    }
    out.push_str(&border);
    if let Some(winner) = winner(view) {
        out.push_str(&format!("{winner} wins\n"));
    }
    Ok(out)
}

fn integer(view: &Map<String, Value>, field: &'static str) -> Result<i64, FrameError> {
    view.get(field)
        .and_then(Value::as_i64)
        .ok_or(FrameError::Missing(field))
}

fn position(
    view: &Map<String, Value>,
    field: &'static str,
    bound: usize,
) -> Result<usize, FrameError> {
    let value = integer(view, field)?;
    usize::try_from(value)
        .ok()
        .filter(|position| *position < bound)
        .ok_or(FrameError::OutsideGrid { field, value })
}
