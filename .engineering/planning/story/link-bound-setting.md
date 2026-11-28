---
format: aep.planning-md/3
id: story:link-bound-setting
kind: story
status: draft
title: The peer link bound is a setting, so its Gate test runs short
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/component.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_processes.rs
- confidence: inferred
  path: website/docs/examples/rally.md
- confidence: cited
  path: website/docs/reference/cli.md
revision: 9
---
## Outcome

The bound within which a peer link must come up (`LINK_WITHIN`, 30 s today) is a setting of
`play --component`: the flag `--link-within <SECONDS>`, read from the setting `link_within`
(`SPINDLE_SETTING_LINK_WITHIN`) where the flag is not given, whole seconds, at least 1, default 30.
With the default every timing is as today. The Gate's test of a peer that never listens runs with a
bound of 1 s and ends within 3 s instead of 30 s.

## Why

Line numbers at b546d6d2.

The test-time budget work of 2026-11-28 (`integration/2026-11-28-1`, a note that is not in this
tree) left one Gate test over 20 s: `a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound`
(`crates/spindle-cli/tests/spindle-cli/adversary_processes.rs:721-745`) waits the product's
`LINK_WITHIN` of 30 s (`crates/spindle-cli/src/component.rs:23`), so it cannot drop below that
without a production change. It took 30.068 s on 2026-11-28 (wave/34 at 42488ced, nextest default
profile, through slot). The Gate nextest step ran 84.5 s at best on a 4-CPU slice against a target
of 80 s.

The step's wall-time gain is not measured: the test's 27 s of test time come off the step's total,
which is between about 3.4 s and 27 s of wall time at 8 test threads, depending on which tests
finish last.

## Acceptance

Each test below runs in the default test step (`task check:the_tests_pass_within_the_budget`,
under 120 s) and ends within 60 s; the story adds no test to the PostgreSQL lane.

- `adversary_processes::a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound`
  (`crates/spindle-cli/tests/spindle-cli/adversary_processes.rs:721-745`), default test step:
  `play --component` started with a link bound of 1 s and a peer that never listens exits non-zero
  naming the table, and the test ends within 3 s (30.068 s before); its wait ceiling (`:737`)
  drops from 45 s to 10 s.
- `adversary_processes::the_link_bound_defaults_to_30_seconds` (new, in the existing module, so
  the test binary's `main.rs` is not edited), default test step: the `link_within` argument of
  `play` in `spindle_cli::cli::Cli::command()` has the default values `["30"]` and the env
  `SPINDLE_SETTING_LINK_WITHIN`; `Cli::try_parse_from` on `play <spec> --environment <file>
  --component <name> --link-within 0` is refused naming `--link-within`, and the same line with
  `--link-within 1` parses; it starts no
  process and ends within 1 s (D1 A, D2 A, D6 A).
- `adversary_processes::a_peer_slow_to_start_is_reached_within_the_connect_bound`
  (`adversary_processes.rs:675`), default test step: passes unchanged on the 30 s default (about
  3.3 s, a 3 s sleep inside).
- `task docs-check`: `play --help` and the generated `website/docs/reference/cli.md` (`:169` and
  the argument table) name `--link-within` and its setting, and say that one bound covers both
  reaching the peers and their links (D3 A, D4 A); `website/docs/examples/rally.md:105-106` names
  the setting; the change goes under `[Unreleased]` `### Added` in `CHANGELOG.md`.
- `task check:the_tests_pass_within_the_budget`: the step ends within 120 s, and the unit's report
  gives its wall time before and after, against the 84.5 s measured and the 80 s target.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W35 A, relayed by the host operator): the scoper's
default, option A, of every decision, each reversible. No new dependency.

- D1 What form the setting takes: A, a flag `--link-within <SECONDS>` on `play` with
  `env = "SPINDLE_SETTING_LINK_WITHIN"` and default 30, like `--console` (`cli.rs:393`); it appears
  in `play --help` and the generated table in `cli.md`, and a placement's `settings` can set it.
  Not taken: B (a setting only, read in `component.rs` as `ingress` is, `:238-265`), C (a plain
  variable `SPINDLE_LINK_WITHIN_MS`, like `SPINDLE_TELEMETRY_PERIOD_MS`).
- D2 Unit: A, whole seconds, at least 1, like `--request-timeout` (`cli.rs:636-644`).
  Not taken: B (milliseconds), C (a duration string such as `2s`).
- D3 "`run --help`" in the acceptance: A, the bound is named in `play --help` and `cli.md` now, the
  only command that links peers (`play --component`); the implementor notes this reading in its
  report, and `story:run-one-component` carries the bound to `run --component` later.
  Not taken: B (also correct the acceptance wording before the work starts), C (wait for
  `story:run-one-component`).
- D4 The help text is wrong about the wait: A, fix it to say one bound covers both phases. Today
  `cli.rs:709` says "up to 30 s" to listen and `:711` "up to 30 s more" for a link, but one
  deadline covers both (`component.rs:144`); `rally.md:106` already says "in all".
  Not taken: B (give each phase its own deadline, which changes behaviour), C (leave the text).
- D5 The fixture's own `LINK_WITHIN` (`crates/spindle-control/src/bin/spindle-control-fixture.rs:24`):
  A, leave it.
  Not taken: B (make it read the same setting).
- D6 How to test the 30 s default: A, check the clap argument's default value
  (`get_default_values() == ["30"]`), which does not depend on the test's environment; changing an
  environment variable in a test is `unsafe` in Rust 2024.
  Not taken: B (call a parsing function with the setting absent), C (a process test that waits
  30 s, over the budget).

## Scope

Derived 2026-11-28 by `story-scoper`, read at 42488ced (wave/34). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli` — cited
- **Files:** `crates/spindle-cli/src/component.rs` — `LINK_WITHIN` `:23`, `Playing` `:32-40`, the one deadline `:144` and the error messages `:154` and `:171` inside `linked` `:113` — cited
- **Files:** `crates/spindle-cli/src/cli.rs` — `Command::Play` description `:695-729` (says "up to 30 s" twice, `:709` and `:711`) and its arguments `:730-751`; the precedent of a flag that is also a setting is `--console` with `env = "SPINDLE_SETTING_CONSOLE"` `:393` — cited
- **Files:** `crates/spindle-cli/src/main.rs:302-318` — the `Command::Play { component: Some(..) }` arm that builds `component::Playing` — inferred (touched only if the setting is a flag, D1 A)
- **Files:** `website/docs/reference/cli.md:169` — generated from `cli.rs` by `spindle-docs` (`task docs-generate`, checked by `docs-check`); not edited by hand — cited
- **Tests:** `crates/spindle-cli/tests/spindle-cli/adversary_processes.rs` — `a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound` `:721-745` (its wait of 45 s at `:737` drops to at most 10 s), and `Proc::spawn` `:130-182`, which clears every `SPINDLE_*` variable (`:160`) and needs a way to pass the bound (10 callers in this file) — cited
- **Tests:** a new default-is-30-s case that reads the clap definition (`spindle_cli::cli::Cli::command()`, as `console_off.rs:17` uses `spindle_cli::cli`), not a process that waits 30 s — inferred
- **Measured (2026-11-28, wave/34 @ 42488ced, nextest default profile, through slot):** `a_peer_that_never_listens…` 30.068 s; its neighbour `a_peer_slow_to_start_is_reached_within_the_connect_bound` 3.342 s (a 3 s sleep inside), so starting the process costs well under 1 s — cited
- **Also likely:** `website/docs/examples/rally.md:105-106` ("up to 30 s in all") names the new setting — inferred
- **Also likely:** `CHANGELOG.md` `## [Unreleased]`, one line — inferred
- **Not touched:** `crates/spindle-control/src/bin/spindle-control-fixture.rs:24` has its own `LINK_WITHIN` (a test stand-in, D5) — cited
- **Documents:** `website/docs/reference/cli.md` (generated), `website/docs/examples/rally.md` — inferred
- **Confidence:** high — the story names `LINK_WITHIN` and the test, and `git grep` gives exactly one product use, `component.rs`
- **Would collide with:** anything editing `spindle-cli`'s `play --component` link set-up (`component.rs`, the `Play` definition in `cli.rs`, the `Play` arm in `main.rs`), the generated `reference/cli.md`, or the `Proc` helper in `adversary_processes.rs`
- **Safety fact:** `LINK_WITHIN` is read only in `linked` (`component.rs:144,154,171`). One deadline covers both connecting and accepting, so with the default left at 30 s every timing is unchanged — step 2, unproven
- **Checked when planned (2026-11-28, read against b546d6d2):** no file under `crates/` changed between 42488ced and b546d6d2. Moved from the scoper's lines: the `Play` description ends at `:729` and its "up to 30 s" lines are `:709` and `:711` (not `:715` and `:718`); the test spans `:721-745` (not `:722-746`) — cited. Under D1 A the `Play` arm in `main.rs` passes the bound to `Playing`, so `main.rs` is edited — cited from the decision. The new default test goes in `adversary_processes.rs`, an existing module, so `crates/spindle-cli/tests/spindle-cli/main.rs` is not edited — inferred. `story:run-one-component` (not in wave 35) shares `component.rs`, `cli.rs` and `main.rs` — cited (its scope entries).
