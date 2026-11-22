---
format: aep.planning-md/3
id: story:bounded-telemetry-collector
kind: story
status: draft
title: The telemetry collector bounds what one component can make it hold
relations:
- serves: vision:spindle
revision: 3
---
# The telemetry collector bounds what one component can make it hold

## Outcome

In 0.17.0 the control process (`crates/spindle-control/src/control.rs:489`, `collect`) spawns
one blocking reader thread per accepted connection, and `FrameReader::next_frame`
(`crates/spindle-telemetry/src/frame.rs:93`) reads a line with no byte limit and no deadline.
After this story:

- A frame longer than a fixed limit (`FRAME_LIMIT`, sized from the largest frame `Sender` writes,
  with margin) ends the connection; the reader never buffers more than the limit plus one read.
- A connection that sends no complete frame within a read deadline (a multiple of the send period)
  is closed.
- At most a fixed number of reader threads run at once (at least the number of placed instances);
  a connection over the limit is closed at accept.
- The channel from readers to the supervisor is bounded; a full channel drops the frame and counts
  the drop, and a reader never blocks on it forever.

## Acceptance

- `an_oversized_frame_ends_its_connection`: a peer that writes `FRAME_LIMIT + 1` bytes without a
  newline is disconnected and the process's resident memory does not grow past the limit.
- `a_silent_connection_is_closed_after_the_deadline`.
- `connections_over_the_reader_limit_are_refused`: opening limit + 10 connections leaves at most
  limit reader threads alive.
- `a_full_frame_queue_drops_and_counts`.
- Existing telemetry and control tests pass unchanged.

## Origin

Spindle 0.17.0 review, 2026-11-22: "Local deployment telemetry".

## Residual

- A local process can hold every reader slot until each connection reaches its deadline;
  refusing peers outside every instance process group at accept would close that.
- `Answers` keeps at most 16 answers (`RECENT_LIMIT`), so a frame has a bound size.
