---
format: aep.planning-md/3
id: story:run-fails-on-rejected-line
kind: story
status: draft
title: A run that rejects an input line or a seed entry exits non-zero
relations:
- serves: vision:spindle
revision: 1
---
# A run that rejects an input line or a seed entry exits non-zero

## Outcome

`run` reading JSON lines on stdin, and `run --seed` before it serves, exit with a non-zero status
when a line or a seed entry was rejected, and name the line. Today a rejected line prints its
failure and the run goes on to exit 0, so a script or simulation that drives Spindle reads success
where some of its input never applied.

Rejected means the line or entry did not run: it is not JSON, names no declared command, its input
is not admitted, its external cause is unanswered, or the kernel refused it before an outcome. A
line answered by a declared outcome, a refusing one included, is not rejected. A seed entry counted
`already-there` is not rejected.

Reported 2026-11-26 by two consumers driving Spindle 0.21.0 from scripts.

## Acceptance

- `a_stdin_run_with_a_rejected_line_exits_1`: lines 1 and 3 run, line 2 names an undeclared command;
  all three print their result, and the run exits 1 after the last line, naming line 2 on stderr.
- `a_declared_refusal_is_not_a_rejection`: a line answered by a declared refusing outcome leaves the
  exit status 0.
- `a_seed_with_a_rejected_entry_stops_the_run`: a seed whose second entry is not admitted exits 1
  before listening, naming the entry, and serves nothing.
