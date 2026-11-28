---
format: aep.planning-md/3
id: story:stdin-sigterm-cleanup-steady
kind: story
status: draft
title: A stdin run removes its temporary state on SIGTERM in every run
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`adversary_unserved::adversary_stdin_run_removes_its_temporary_state_on_sigterm` passes in 20 of 20
runs inside the full `spindle-cli` test run, and the stdin run removes its temporary state on
SIGTERM whatever the store is doing when the signal comes.

## Why

Wave 32's U3 implementor saw the test fail once in a full `cargo test -p spindle-cli` run on
`unit/w32-sigterm` and pass 8 of 8 alone. The stdin path in `main.rs`, which U3 did not change,
removes the temporary directory from the signal handler; the suspected race is the store creating
its file in that directory after the removal. Not diagnosed.

## Acceptance

- A red-capable reproduction first (`aep:diagnosing`), or a written finding that none was found in
  50 runs under load.
- If reproduced: the cause in the `CHANGELOG.md` entry, and the test passes 20 of 20 runs in the
  full crate run.
