---
format: aep.planning-md/3
id: story:clock-and-mint-in-every-mode
kind: story
status: implemented
title: Every run mode takes a simulated clock and a seeded mint
relations:
- serves: vision:spindle
- depends_on: story:run-fails-on-rejected-line
revision: 17
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T18:58:24Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "proposed", to: "active", at: "2026-11-26T18:58:24Z", actor: "human:operator", revision: 16, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "active", to: "implemented", at: "2026-11-26T22:17:36Z", actor: "human:operator", revision: 17, decided_on: {"recorded":{"test_result":1,"review_outcome":15}}}
---
# Every run mode takes a simulated clock and a seeded mint

## Outcome

`run` in each mode (stdin lines, HTTP, `--scenario`) takes `--clock <RFC 3339>`, so the run starts on
a manual clock at that instant. Stdin and HTTP runs also take `--mint-seed <n>`, so identities and
generated values are minted from that seed from the first command. On stdin a line may carry
`"at": "<RFC 3339>"`, which moves the clock to that instant before the command runs; an instant
before the current one rejects that line.

Today a seeded mint exists only behind `POST /api/reset` (`"mint": <seed>`), stdin mode mints random
UUIDs, and every mode reads the system clock. Reported 2026-11-26 by two consumers that simulate a
business over months: per-month figures need the clock under their control, and repeatable runs
need repeatable identities.

## Acceptance

- `a_seeded_stdin_run_mints_the_same_identities_twice`: two stdin runs with one `--mint-seed` print
  byte-identical results, minted identities included; a different seed mints different ones.
- `a_line_moves_the_clock`: with `--clock 2026-01-01T00:00:00Z`, a line with `"at":
  "2026-02-01T00:00:00Z"` runs a command whose guard orders against `now` and answers the outcome
  selected on February 1st; the same command on a run without `--clock` answers the other outcome
  (the guard is `now < 2026-06-01T00:00:00Z`: true on February 1st, false on the system clock
  of any day the test runs on, which is after 2026-06-01).
- `a_line_with_an_earlier_at_is_rejected`: after that line, a line with `"at":
  "2026-01-15T00:00:00Z"` is rejected and the run exits 1 (story:run-fails-on-rejected-line).
- `a_served_run_mints_from_the_seed`: two served runs with one `--mint-seed` answer the same
  identity to the same first create request.
- `a_served_run_starts_on_the_clock_flag`: a served run with `--clock 2026-02-01T00:00:00Z` answers
  a command whose guard orders against `now` with the outcome selected on February 1st, and a served
  run without `--clock` answers the other outcome (the same guard).
- `a_scenario_runs_on_the_clock_flag`: `--scenario` with `--clock 2026-02-01T00:00:00Z` replays an
  act that states the outcome selected on February 1st by a guard over `now`; the act's line
  contains `holds` and the run exits 0, and the same document without `--clock` exits 1; the test
  asserts the exit code and the `holds` substring only, since U4 adds the observed outcome to the
  line.
- `a_reset_world_mint_overrides_the_seed_until_the_next_reset`: on a served run with
  `--mint-seed`, a reset whose world carries `mint: <m>` mints as seed `<m>` does.
- `a_plain_reset_mints_from_the_run_seed_again`: a following reset whose world has no `mint` mints
  the same identities as the run's first commands did.
- `a_seeded_mint_binding_is_served`: a stdin run on an environment that binds `identity.mint` to
  `seeded`, which `admit` already accepts, runs instead of being refused, and mints from
  `--mint-seed` (seed 0 when the flag is absent); two such runs print byte-identical results.

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

- `crates/spindle-cli/src/main.rs`, `seed.rs`, `serve.rs`: shared with
  `story:run-fails-on-rejected-line` in the same unit (U3), built after it on one branch
  (`depends_on`).
- `crates/spindle-conformance/src/target.rs`: `HostTarget::new` keeps its signature; this story adds
  a separate constructor that takes a clock and a mint, so the about 30 callers of `new` do not
  change. U4 adds a field to `HostTarget` (the outcomes taken); the coordinator merges U3 first and
  U4's field joins the new constructor at merge.
- `crates/spindle-cli/src/replay.rs`: this story changes only the host it opens in `Replays::run`
  (`:187`) to the new constructor. U4 edits `replay.rs:44-153` and the line printing in
  `Replays::run`; shared file, separate hunks, merged by the coordinator after U3.
- `crates/spindle-cli/tests/spindle-cli/replay.rs`: this story adds one test module function for
  `--scenario` with both flags; U4 adds its own. Shared file, separate functions.
- `crates/spindle-host/src/host.rs` (`Opening`, `:984`, `reset_to` `:1195-1236`): shared with U2,
  which edits only the leaf-list doc at `:1625-1645`. Separate hunks, merged by the coordinator.
- The `run` help in `cli.rs` and the generated `cli.md`: shared with U4; separate hunks, the page
  generated again after both.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.

## Not covered

- Periodic bindings: no `run` mode fires them today (only `play` does), and this story does not add
  it (`a_periodic_binding_fires_as_the_clock_passes`, moved here when scoped).
- `--mint-seed` on `--scenario`: ESS's runner captures identities itself and no output of the
  replay shows a minted identity, so no test could observe it.
- Stepping the clock in HTTP and `--scenario` runs: they take a start instant from `--clock` only.
  A scenario act's `at:` stays with ESS's runner.
- The environment's `clock` binding: it is not checked today and this story does not check it;
  `--clock` sets the time.
