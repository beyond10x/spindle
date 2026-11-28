# Wave 35

**Proposed.** Planned on the operator's pick of 2026-11-28 (D-W35 A, relayed by the host operator); story:postgres-lane-margin-ci left the wave when its cut was found to land in eventlog. All stories serve vision:spindle.

Integration branch `wave/35`, forked from `main` after wave 34 (PR 110) lands. Release 0.29.0
(wave 34) comes first; release 0.30.0 follows this wave. This plan was written on `plan/wave-35` at
b546d6d2 (wave 34's branch, carrying `main` at release 0.28.0); every line number in the stories is
read there. No file under `crates/`, `ess/` or `ui/` differs between it and 42488ced, where the
scopers read.

## Selection

The coordinator chose the scope under the host operator's standing rule (from accepted epics,
budget fixes first):

- `story:link-bound-setting` (U1): a test-budget fix. The Gate's test step ran 84.5 s against its
  80 s target, and `a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound` takes
  30.068 s because it waits the product's fixed 30 s link bound.
- `story:observe-label-vocabulary` (U2): the first step of the observability, console and
  Kubernetes chain (`epic:runtime-observability-depth`); eight drafts `depends_on` it.
- `story:unknown-commit-in-seed-and-retries` (U3): wave 34's follow-up (D4 A of
  `story:unknown-commit-answered-as-unknown`): a bound command or seed entry whose commit outcome
  is unknown is sent again today.

U1 and U3 were scoped by one `story-scoper` each at 42488ced, their lines read again at b546d6d2.
U2 had a Scope section and scope entries (revision 3) but no scoper report; its Scope was checked
file by file against b546d6d2 and its Notes turned into decisions D1-D4. Each story's `## Scope`
holds its surface, its `## Decided when scoped` the options taken, and its `## Outcome`, `## Why`
and `## Acceptance` were amended where a line was stale or a decision changed it.

Not in the wave; each stays a draft:

- `story:postgres-lane-margin-ci`: waits on an eventlog release where opening a current schema
  costs no DDL (its `## Waits on`). Its diagnosis of 2026-11-28 put 185 s of the lane's 294 s of
  statement time in eventlog's migration on each of 1,459 store opens.
- `story:request-id-idempotency`: how request ids are kept is undecided.
- `story:run-one-component`: shares `crates/spindle-cli/src/component.rs` (and `cli.rs`,
  `main.rs`) with U1.

Acceptance amendments, one line per story:

- `story:link-bound-setting`: `--link-within <SECONDS>` on `play` (`SPINDLE_SETTING_LINK_WITHIN`,
  default 30); the never-listens test runs with 1 s and ends within 3 s; a new clap-definition test
  holds the 30 s default; help and `cli.md` say one bound covers both phases.
- `story:observe-label-vocabulary`: 18 families, not 14; existing labels unchanged (D1 A); a new
  unit test `the_label_vocabulary_is_the_specified_vocabulary` reads the vocabulary and its label
  rule from the generated schema and fails on today's tree; the unit test
  `the_families_are_the_specified_families` compares `level` and `unit`, `unit` required and
  `"none"` for a family that counts things (D5 A); the fixture validates against the regenerated
  schema.
- `story:unknown-commit-in-seed-and-retries`: named host, ingress, seed and serve unit tests for
  D1-D6; a store error other than `Unknown` keeps today's retry and `Rejected`; the two concept
  pages hold named sentences, and `run --help` and the regenerated `cli.md` say an entry whose
  outcome is unknown is recorded done, counted under `rejected` and not sent again by a run that
  keeps a seed record.

## Units

| unit | story | surface | branch | tree |
|---|---|---|---|---|
| U1 | `story:link-bound-setting` | `crates/spindle-cli/src/component.rs` (`LINK_WITHIN`, `Playing`, `linked`), `cli.rs` (`Command::Play`), `main.rs` (the `Play` arm), `tests/spindle-cli/adversary_processes.rs`, `website/docs/reference/cli.md` (generated), `website/docs/examples/rally.md` | `unit/w35-link-bound` | `spindle-w35-link-bound` |
| U2 | `story:observe-label-vocabulary` | `ess/domains/observe.yaml` (`MetricFamily`, `MetricFamilies`), `ess/fixtures/observe/metrics/families.json`, `crates/spindle-ingress/src/observe/metrics.rs` (`Family`, `FAMILIES`), generated `ess/generated/jsonschema`, `crates/spindle-observe-api`, `ui/src/api/generated`, `docs/design/observe.md` | `unit/w35-label-vocabulary` | `spindle-w35-label-vocabulary` |
| U3 | `story:unknown-commit-in-seed-and-retries` | `crates/spindle-host/src/host.rs` (`Host::attempt`), `crates/spindle-kernel/src/store.rs` (`DeadLetter`), `crates/spindle-cli/src/seed.rs`, `serve.rs` (`did_not_run`), `crates/spindle-ingress/src/console.rs` (`replay`, `letter`), `ess/domains/console.yaml` and `observe.yaml` (`DeadLetter`, `SeedState`), the same generated files as U2, new `tests/spindle-host/unknown_retry.rs` and its `mod` line, `tests/spindle-ingress/unknown_commit.rs`, `components-and-bindings.md`, `environment-and-admission.md`, `crates/spindle-cli/src/cli.rs` (the `--seed` help of `run`), `website/docs/reference/cli.md` (generated) | `unit/w35-unknown-seed-retries` | `spindle-w35-unknown-seed-retries` |

Two `depends_on` edges inside the wave, both added at review round 1:
`story:unknown-commit-in-seed-and-retries depends_on story:observe-label-vocabulary` (U3 reads the
generated files U2 regenerates) and `story:unknown-commit-in-seed-and-retries depends_on
story:link-bound-setting` (U3 regenerates `cli.md` on top of U1's change to `cli.rs`). They record
the merge order; the units are still built in parallel. Each unit is built by one
`aep:implementor` and gets one `aep:adversary` pass when green.

## Surfaces

Files two or more units share, and how they are kept apart:

- `CHANGELOG.md` `[Unreleased]`: all three. U1 and U2 under `### Added`, U3 under `### Fixed`; each
  unit adds its own bullet naming its story id; each merge into `wave/35` keeps both sides, ordered
  by story id.
- `ess/domains/observe.yaml`: U2 (`MetricKind`, `MetricFamily`, `MetricFamilies`, `:517-546`, and
  the new vocabulary beside them) and U3 (`DeadLetter` `:876-927`, `SeedState.rejected`
  `:983-987`). Different types; U3 merges after U2 and keeps both sides.
- Generated from `ess/` by `task generate:observe`, never edited by hand:
  `ess/generated/jsonschema` (its `.ess-output/state.json` changes on every regeneration),
  `crates/spindle-observe-api` and `ui/src/api/generated` (`source.schema.json`, the types,
  `types-report.json`, `.ess-output/state.json`): U2 and U3. U3 merges after U2, and its implementor
  regenerates these files on top of U2's merged change before its last round, so
  `task check:the_generated_types_have_no_drift` holds both changes.
- `crates/spindle-cli/src/cli.rs`: U1 and U3. U1 edits `Command::Play` (`:695-751`: the
  `--link-within` argument and the help's two "up to 30 s" lines); U3 edits the `--seed` help of
  `run` (`:536-585`: "rejected (it did not run: …)" `:547-548` and "a restart with the same file
  sends the rejected entries again" `:559-561`). Different variants; U3 merges after U1 and keeps
  both sides.
- `website/docs/reference/cli.md`, generated from `cli.rs` by `task docs-generate` and checked by
  `task docs-check`, never edited by hand: U1 (`play`, `:163-186`) and U3 (the `--seed` row of
  `run`, `:153`). U3 merges after U1, and its implementor regenerates `cli.md` on top of U1's merged
  change before its last round, so `task docs-check` holds both changes.
- `crates/spindle-cli/tests/spindle-cli/main.rs`: not shared. U1 puts its new test in the existing
  `adversary_processes.rs` and adds no module; U2 and U3 add no `spindle-cli` integration test
  (U3's seed and serve cases are unit tests inside `seed.rs` and `serve.rs`).

No other file is in two units' scope.

## Order and parallel safety

All three are built in parallel from `wave/35`. Merge order: U1, U2, U3, recorded by U3's two
`depends_on` edges. U3 merges after U1 and U2, and regenerates `cli.md` on top of U1's change and
the observe files on top of U2's.

- Default test step (`task check:the_tests_pass_within_the_budget`, under 120 s): U1 cuts about
  27 s of test time (the never-listens test from 30.068 s to within 3 s) and adds one test under
  1 s; U2 adds one unit test and changes one, each under 1 s; U3 adds seven tests, each under 1 s, about 1.3 s in
  all. U1 reports the step's wall time before and after, against 84.5 s and the 80 s target.
- PostgreSQL lane (`task check:postgres`): no unit adds a lane test. The lane ran 89.8 s (PR 109)
  and 98.5 s (PR 108) of its 100 s bound on CI; its margin waits on eventlog
  (`story:postgres-lane-margin-ci`).

## Decisions taken

Every decision takes option A, each reversible (D-W35 A).

- `story:link-bound-setting`: D1 A (flag `--link-within` with `env = "SPINDLE_SETTING_LINK_WITHIN"`,
  default 30), D2 A (whole seconds, at least 1), D3 A (named in `play --help` and `cli.md` now;
  `story:run-one-component` carries it to `run`), D4 A (help text says one bound covers both
  phases), D5 A (the control fixture's own `LINK_WITHIN` left), D6 A (the default checked on the
  clap definition).
- `story:observe-label-vocabulary`: D1 A (existing families keep their labels; the exposition does
  not change), D2 A (`domain`, not `context`), D3 A (the families the level list does not name take
  one of its ten levels), D4 A (built beside U3, merged first; U3 `depends_on` it), D5 A (`unit`
  required on every family, `"none"` for a family that counts things; taken at review round 1).
- `story:unknown-commit-in-seed-and-retries`: D1 A (stop, and keep a dead letter marked unknown),
  D2 A (`"committed": "unknown"` in the dead-letter listing), D3 A (`result: "unknown"` on a
  replay), D4 A (escalation unchanged, left to a follow-up), D5 A (the seed entry recorded done,
  the marker format unchanged), D6 A (`Became::Unknown`, counted under `rejected`, exit 1 at the
  seed's end), D7 A (PostgreSQL without a lease unchanged, documented); at review round 1, U3
  updates the `--seed` help of `run` in `cli.rs` and regenerates `cli.md` on top of U1.

## What a unit must not do

- Write Python in anything that stays in the repository: tools, tasks, gates, CI steps and test
  helpers are Rust or POSIX sh.
- Add a test that runs over 60 s: nextest kills it (`.config/nextest.toml`, `slow-timeout` with
  `terminate-after`). A test over 5 s is named with its time in the unit's report.
- Let a test step run over 120 s, or add a test to the PostgreSQL lane.
- Hold a pool job past 10 minutes; a job that needs longer names its time with `slot --timeout`,
  at most 30 minutes. A script that waits or measures starts without the `slot` prefix; `cargo`
  inside it takes a slot through the shim.
- Run the whole workspace's tests; run the tests of the crates the unit touched.
- Make a model call, a call to a real API or a network request in a test; loopback and stub stores
  only.
- Edit a generated file by hand: `ess/generated/jsonschema`, `crates/spindle-observe-api`,
  `ui/src/api/generated` and `website/docs/reference/cli.md` come from their generators.
- Add a dependency.
