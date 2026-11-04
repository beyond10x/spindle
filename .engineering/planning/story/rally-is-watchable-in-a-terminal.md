---
format: aep.planning-md/3
id: story:rally-is-watchable-in-a-terminal
kind: story
status: implemented
title: Watch a rally match in a terminal
relations:
- decomposes: epic:m1-m3-rally-in-three-vms
- depends_on: story:execute-commands
- depends_on: story:periodic-bindings-fire
- depends_on: story:bindings-cross-components
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-host
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-04T07:28:12Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-04T10:52:46Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# Watch a rally match in a terminal

## Outcome

`b10x-spindle play examples/rally/ess --environment examples/environments/laptop.toml` runs the
three rally components in one process: the timer drives `Tick`, in-process channels carry
`BallMoved` and `PaddleMoveRequested`, and the terminal redraws the 16×9 grid after each tick —
ball, both paddles, points still needed — until a side wins. The grid is drawn from the table's
`MatchState` view, never from private state.

## Acceptance

- `a_match_played_in_one_process_ends_with_a_winner` — with a test clock, no terminal.
- `the_frame_shows_the_ball_and_both_paddles_where_match_state_says` — renders one frame to a
  string.
- `play_refuses_an_environment_without_a_timer`.

## Scope

`crates/spindle-host/` (new: composes kernel, timer and channel), `crates/spindle-cli/src/main.rs`.
