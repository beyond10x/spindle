---
format: aep.planning-md/3
id: story:clock-and-mint-in-every-mode
kind: story
status: draft
title: Every run mode takes a simulated clock and a seeded mint
relations:
- serves: vision:spindle
revision: 5
---
# Every run mode takes a simulated clock and a seeded mint

## Outcome

`run` in each mode (stdin lines, HTTP, `--scenario`) takes `--mint-seed <n>`, so identities and
generated values are minted from that seed from the first command, and `--clock <RFC 3339>`, so the
run starts on a manual clock at that instant. On stdin a line may carry `"at": "<RFC 3339>"`, which
moves the clock to that instant before the command runs; an instant before the current one is
refused for that line. Periodic bindings fire as the clock passes their times, as `ManualClock`
drives them natively (`Host::advance`).

Today a seeded mint exists only behind `POST /api/reset` (`"mint": <seed>`), stdin mode mints random
UUIDs, and every mode reads the system clock. Reported 2026-11-26 by two consumers that simulate a
business over months: per-month figures need the clock under their control, and repeatable runs
need repeatable identities.

## Acceptance

- `a_seeded_stdin_run_mints_the_same_identities_twice`: two stdin runs with one `--mint-seed` print
  byte-identical results, minted identities included; a different seed mints different ones.
- `a_line_moves_the_clock`: with `--clock 2026-01-01T00:00:00Z`, a line with `"at":
  "2026-02-01T00:00:00Z"` runs a command whose guard orders against `now` at February 1st, and a
  later line with an earlier `at` is rejected.
- `the_served_run_and_the_scenario_take_both_flags`: HTTP and `--scenario` runs honour
  `--mint-seed` and `--clock`; a reset world's `mint` still overrides the seed until the next reset.
- `a_plain_reset_mints_from_the_run_seed_again`: on a served run with `--mint-seed`, a reset whose
  world has no `mint` mints the same identities as the run's first commands did.

Moved to Not covered when scoped: `a_periodic_binding_fires_as_the_clock_passes`. No `run` mode
fires periodic bindings today (only `play` does), and this story does not add it.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` in `run`'s three modes, and `crates/spindle-host` for a run seed that outlives a plain reset — cited
- **Files:** `crates/spindle-cli/src/cli.rs:183-322` (`Run` arguments; `--scenario` uses `conflicts_with_all` at :321, so the new flags must not conflict with it) — cited
- **Files:** `crates/spindle-cli/src/main.rs:466-717` (`execute`, stdin loop :696-716), `:740-780` (`answer` calls `execute_as` with `UuidV4` and `SystemTime::now`, :758-765) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:367-375` (`Host::with_stores(.., Arc::new(SystemClock), ..)` at :370) — cited
- **Files:** `crates/spindle-cli/src/replay.rs:173-205` and `crates/spindle-conformance/src/target.rs:91-130` (`HostTarget::host` opens the host on `SystemClock` and the default `UuidV4`, :121-130; ESS's runner clock and ids never reach it) — cited
- **Files:** `crates/spindle-host/src/host.rs:367-371` (`Opening`), `:984` (`mint: Box::new(UuidV4)`), `:1195-1236` (`reset_to`: a world without `mint` installs `UuidV4` again, :1232-1235) — cited
- **Symbols:** `Host::with_stores`, `Host::reset_to`, `World::mint`, `spindle_kernel::Seeded`, `spindle_kernel::UuidV4`, `spindle_kernel::execute_as`, `ManualClock::at`, `ManualClock::advance`, `SystemClock`, `HostTarget::host` — cited
- **Also likely:** a run seed held on `Host` that `reset_to` falls back to; a clock and mint parameter on `HostTarget::new`; tests in `crates/spindle-cli/tests/spindle-cli/run.rs`, `mint.rs`, `replay.rs`, `served_world.rs` — inferred
- **Documents:** `README.md` (run flags), the `run` doc in `cli.rs` and the generated `website/docs/reference/cli.md`, `CHANGELOG.md` — inferred
- **Not touched:** `crates/spindle-kernel` (`execute_as` already takes `&mut dyn Mint` and a `now`; `Seeded` exists at mint.rs:516-600), `crates/spindle-timer`, `crates/spindle-embed`, `crates/spindle-node` — inferred
- **Shares code with:** story:node-export-clock-and-seeds needs the same host-held seed (host.rs:367-371, :984, :1232-1235); neither depends on the other — inferred
- **Confidence:** medium — the CLI sites are cited; where the run seed lives in `Host` is decided below
- **Would collide with:** the stdin loop in `main.rs` (story:run-fails-on-rejected-line), `serve.rs` and `seed.rs` (story:seed-once-on-durable-state), the `Run` doc in `cli.rs` and its generated page
- **Gap confirmed:** stdin mints `UuidV4` and reads `SystemTime::now` (main.rs:762-763); serve uses `SystemClock` (serve.rs:370) and seeds only on a reset world (host.rs:1218-1235); `--scenario` opens the host on `SystemClock` with `UuidV4` (target.rs:126, host.rs:984); no `run` mode calls `Host::start` or `Host::advance` (only `play` does, main.rs:328-372) — cited

## Decided when scoped

- The run's seed lives on `Host` beside its mint; a reset whose world has no `mint` installs
  `Seeded` from the run's seed again, from its first draw, so every plain reset repeats the same
  identities. A world's `mint` wins until the next reset.
- `"at"` on a stdin line moves a `ManualClock` forward by `at - now`; an earlier `at` rejects the line
  (with story:run-fails-on-rejected-line, a rejection makes the run exit 1 at its end).
- Periodic bindings are not fired by any `run` mode today, and this story does not add it: the
  acceptance case `a_periodic_binding_fires_as_the_clock_passes` moves to Not covered.
- `--scenario` takes both flags into the host it opens (`HostTarget`); ESS's runner keeps its own
  clock and ids for the document.

## Shared files

- `crates/spindle-cli/src/main.rs`, `serve.rs`: shared with `story:run-fails-on-rejected-line` in
  the same unit (U3), built after it on one branch.
- `crates/spindle-conformance/src/target.rs` (`HostTarget::new` takes a clock and a mint): shared
  with U4 (`story:scenario-act-without-outcome` records the outcome taken in `execute_command`);
  separate functions, merged by the coordinator.
- `crates/spindle-host/src/host.rs` (`reset_to`, the run seed): this story's alone in wave 26.
- The `run` help in `cli.rs` and the generated `cli.md`: shared with U4, as above.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.
