---
format: aep.planning-md/3
id: story:unknown-commit-in-seed-and-retries
kind: story
status: draft
title: A bound command or seed entry whose commit outcome is unknown is not sent again
summary: The bound-command retry and the seed treat StoreError::Unknown as an outcome to check, not a command that did not run.
relations:
- serves: vision:spindle
- depends_on: story:observe-label-vocabulary
- depends_on: story:link-bound-setting
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/main.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/unknown_retry.rs
- confidence: cited
  path: crates/spindle-ingress/src/console.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/unknown_commit.rs
- confidence: inferred
  path: crates/spindle-kernel/src/store.rs
- confidence: inferred
  path: crates/spindle-observe-api
- confidence: inferred
  path: docs/design/spindle.md
- confidence: cited
  path: ess/domains/console.yaml
- confidence: inferred
  path: ess/domains/observe.yaml
- confidence: cited
  path: ess/generated/jsonschema
- confidence: inferred
  path: ess/generated/jsonschema/schema/types/spindle.console.ReplayResult.schema.json
- confidence: inferred
  path: ui/src/api/generated
- confidence: cited
  path: website/docs/concepts/components-and-bindings.md
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: cited
  path: website/docs/reference/cli.md
revision: 13
---
## Outcome

A bound command or a seed entry whose commit outcome is unknown (`StoreError::Unknown`, from
`story:unknown-commit-answered-as-unknown`) is not treated as a command that did not run:

- a bound command under `on_failure: retry` is not sent again: it stops after that attempt and is
  kept as a dead letter marked unknown, which `GET /api/dead-letters` lists with
  `"committed": "unknown"` (D1 A, D2 A); a replay that meets an unknown outcome answers `200` with
  `result: "unknown"` and the id of its new, marked dead letter (D3 A);
- a seed entry is not marked rejected and sent again at the next start: it is recorded done, its
  request id remembered, and the run logs at error level that its outcome is unknown and that it
  may have been written; it counts under `rejected`, and the run exits 1 at the seed's end as for a
  rejected entry (D5 A, D6 A).

An operator can so check before anything is sent twice. Every other store error keeps today's retry
and today's `Rejected`; an escalating binding still escalates (D4 A).

## Why

Line numbers at b546d6d2, which holds wave 34 (`StoreError::Unknown`,
`crates/spindle-kernel/src/store.rs:351`; `Answer::unknown`, `crates/spindle-ingress/src/answer.rs:110`).
Found while scoping `story:unknown-commit-answered-as-unknown` (its D4 A left these paths out). Both
are reachable on PostgreSQL, the only store that returns an unknown commit.

- Bound commands: the retry loop in `Host::attempt` (`crates/spindle-host/src/host.rs:2694`, its loop from
  `:2722`, the retry test `:2749-2754`) counts any `Err` from `execute` as a refusal (`Err(_) => Some(None)`, `:2747`) and, under
  `on_failure: retry`, sends the command again, keeping the last attempt as a dead letter
  (`lettered` `:2764`, the `Err` arm `:2818-2831`). A commit that may have landed is retried as if
  it had not.
- Seed: `crates/spindle-cli/src/seed.rs:1111` marks an entry whose command failed with any
  `HostError::Execute` as `Rejected`, and a restart sends it again.

## Acceptance

Each test below runs in the default test step (`task check:the_tests_pass_within_the_budget`) on
stub or memory stores and ends within 1 s; the story adds about 1.3 s of test time to the step and
no test to the PostgreSQL lane.

- `unknown_retry::a_bound_command_whose_commit_is_unknown_is_tried_once_and_kept_marked_unknown`
  (new, `crates/spindle-host/tests/spindle-host/unknown_retry.rs`; D1 A): on the `retry` fixture
  (`record-filing`, `on_failure: retry`, `crates/spindle-host/tests/fixtures/retry/system.yaml:37-45`)
  with a supplied store whose `commit` and `commit_all` fail `StoreError::Unknown`, the bound command
  `filing.ledger.Record` is attempted once (`Failed::attempts` 1), no `Retried::Exhausted` is
  counted, and its dead letter is kept marked unknown.
- `unknown_retry::a_bound_command_whose_store_fails_otherwise_is_retried_as_before` (new, same
  module): with `StoreError::Backend` instead, the command is attempted `RETRY_ATTEMPTS` times,
  `Retried::Exhausted` is counted, and its dead letter carries no mark.
- `unknown_commit::the_dead_letter_listing_marks_an_unknown_commit` (new, in the existing
  `crates/spindle-ingress/tests/spindle-ingress/unknown_commit.rs`; D2 A): `GET /api/dead-letters`
  lists that letter with `"committed": "unknown"`, and a letter of a refused command without the
  key.
- `unknown_commit::a_replay_whose_commit_is_unknown_answers_result_unknown` (new, same module;
  D3 A): `POST /api/dead-letters/replay` of a letter whose replay meets an unknown commit answers
  `200` with `result: "unknown"` and the id of the new dead letter, which the listing marks unknown.
- `seed::tests::an_entry_whose_commit_is_unknown_is_done_and_not_sent_again` (new `#[cfg(test)]`
  module in `crates/spindle-cli/src/seed.rs`; D5 A, D6 A): with a stub store whose commit fails
  `StoreError::Unknown`, the marker records the entry done and not in `rejected`; a second seeding
  over the same marker does not submit it; the run logs at error level that its outcome is unknown
  and that it may have been written, counts it under `rejected`, and the seed ends failed (exit 1).
- `seed::tests::an_entry_rejected_otherwise_is_sent_again` (new, same module): an entry whose store
  fails `StoreError::Backend` is recorded `Rejected`, as today, and the next seeding submits it
  again.
- `serve::tests::the_line_of_a_command_whose_outcome_is_unknown_says_it_may_have_run` (new, in the
  unit tests of `crates/spindle-cli/src/serve.rs`): the line `did_not_run` (`:1013-1036`) prints
  for such a command says its outcome is unknown, not that it did not run.
- The existing escalation cases pass unchanged: an escalating binding still escalates on an unknown
  outcome (D4 A).
- `task check:the_observe_domain_validates` and `task check:the_generated_types_have_no_drift` pass
  with `committed` on `spindle.observe.DeadLetter`, `unknown` in `spindle.console.ReplayResult` and
  the `SeedState.rejected` summary changed, regenerated on top of `story:observe-label-vocabulary`'s
  merged change.
- `task docs-check` passes, and the two pages hold these statements:
  - `website/docs/concepts/components-and-bindings.md`, § "Dead letters", its first paragraph
    (`:52-57`, `on_failure` and the kept dead letter): an outcome the store reports as unknown is
    kept as a dead letter marked unknown and is not retried or sent again (D1 A).
  - `website/docs/concepts/environment-and-admission.md`, § "A seed submitted at start", the
    `rejected` row of the count table (`:260`), and § "A seed on a kept state", its second bullet
    (`:293-296`, the entries rejected before are sent again): an entry whose outcome is unknown is
    recorded done, counted under `rejected`, and not sent again by a `--state` run or a leased run
    (D5 A, D6 A).
  - The same page, the paragraph of § "A seed on PostgreSQL with a lease" that ends "Any other
    store, in PostgreSQL without `database_lease` or in memory, records nothing" (`:351-375`):
    without a lease on PostgreSQL there is no seed record, so such an entry is sent again at the
    next start (D7 A, `seed.rs:44-45`).
- `task docs-check` passes with `website/docs/reference/cli.md` regenerated by `task docs-generate`
  on top of `story:link-bound-setting`'s merged change, and `run --help` (the command that carries
  `--seed`, `crates/spindle-cli/src/cli.rs:536-585`) shows the new sentence: an entry whose outcome
  is unknown is recorded done, counted under `rejected`, and not sent again by a run that keeps a
  seed record. It replaces the help's "rejected (it did not run: …)" (`:547-548`) as the whole
  meaning of `rejected`, and its "a restart with the same file sends the rejected entries again"
  (`:559-561`) no longer covers such an entry. The generated row is the `--seed` row of `run` in
  `cli.md` (`:153`).
- The change goes under `[Unreleased]` `### Fixed` in `CHANGELOG.md`.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W35 A, relayed by the host operator): the scoper's
default, option A, of every decision, each reversible. No new dependency.

- D1 What a bound command does after an unknown commit outcome: A, stop at once, with no further
  attempt and no `exhausted` count, and keep the command as a dead letter marked unknown, since the
  letter holds the input the operator needs; a replay is the operator's explicit word to send it
  again.
  Not taken: B (stop and keep no dead letter, only the log line names it; the input is lost, since
  it is never logged), C (read back whether the commit landed before retrying; Spindle passes
  eventlog no command identity it can query, and a test would need the PostgreSQL lane).
- D2 How `GET /api/dead-letters` shows the mark: A, `"committed": "unknown"`, the key commands
  already answer with; `spindle.observe.DeadLetter` and the generated types gain the field.
  Not taken: B (`error` set to a new value `"store_unknown"`, which breaks its documented meaning
  "as `spindle_command_failures_total` labels it"), C (store the mark and leave the listing
  unchanged, so the operator cannot see it).
- D3 What a replay answers when its own run meets an unknown outcome (`console::replay`, today
  `result: "failed"`, `console.rs:753`): A, `200` with `result: "unknown"`, a new
  `spindle.console.ReplayResult` variant, and the id of the new, marked dead letter.
  Not taken: B (`500` `{"refused", "committed": "unknown"}` through `Answer::unknown`, losing the
  new dead letter's id), C (keep `"failed"`).
- D4 Escalation (`on_failure: {escalate}` or `on_refusal`) on an unknown outcome: A, unchanged; it
  still escalates. The story names only retry and seed; escalation is left to a follow-up story.
  Not taken: B (do not escalate; keep a marked dead letter instead).
- D5 The seed marker for an unknown entry: A, record the entry done, not rejected, and remember its
  request id, so neither this run nor a restart sends it; the marker format stays as it is
  (`Recorded` is `deny_unknown_fields`, `seed.rs:179-185`).
  Not taken: B (a new `unknown` set in the marker, on which a rollback to an older binary would
  stop), C (record it rejected and resend it, today's behaviour, which the story rules out).
- D6 What seed state and readiness report: A, a new `Became::Unknown`, counted under `rejected`
  and logged at error level ("outcome unknown: it may have been written; check before sending it
  again"); the run exits 1 at the seed's end, as for a rejected entry, and readiness is unchanged
  (`200` once ended). The `SeedState.rejected` summary (`observe.yaml:983-987`) and the table row
  `environment-and-admission.md:260` change.
  Not taken: B (a new `unknown` count in `SeedState`, the end line and
  `spindle_seed_commands_total`, all on `story:observe-label-vocabulary`'s files), C (count it
  under `rejected` but keep serving, no exit 1, and change no observe text).
- D7 PostgreSQL without `database_lease` keeps no seed record and sends the whole seed at every
  start (`seed.rs:44-45`): A, unchanged; the documents say that only `--state` and leased runs keep
  the entry from being sent again.
  Not taken: B (keep a record there too, outside this story).

Taken at the wave's review round 1: this story updates the `--seed` help of `run`
(`crates/spindle-cli/src/cli.rs:536-585`), which says a rejected entry "did not run" (`:547-548`)
and that a restart sends the rejected entries again (`:559-561`); D5 A and D6 A make both wrong for
an entry whose outcome is unknown. The new text says such an entry is recorded done, counted under
`rejected`, and not sent again by a run that keeps a seed record. Its generated copy,
`website/docs/reference/cli.md` (`:153`), is regenerated with `task docs-generate`, never edited
by hand.

Wave 35 order: this story `depends_on` `story:observe-label-vocabulary` (U2) and
`story:link-bound-setting` (U1), and merges after both.

- With U2 it shares `ess/domains/observe.yaml`, `crates/spindle-observe-api`,
  `ui/src/api/generated` and `ess/generated/jsonschema`. Its implementor regenerates the generated
  files (`task generate:observe`) on top of U2's merged change before its last round.
- With U1 it shares `crates/spindle-cli/src/cli.rs` (U1 edits `Command::Play`, `:695-751`; this
  story the `--seed` help of `run`, `:536-585`) and the generated `website/docs/reference/cli.md`.
  Its implementor regenerates `cli.md` with `task docs-generate` on top of U1's merged change
  before its last round.

## Scope

Derived 2026-11-28 by `story-scoper`, read at 42488ced (wave/34, which holds
`story:unknown-commit-answered-as-unknown` and `StoreError::Unknown`). Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong).

- **Depends on:** `story:unknown-commit-answered-as-unknown` (wave 34): `StoreError::Unknown`
  (`crates/spindle-kernel/src/store.rs:351`), `Answer::unknown` (`crates/spindle-ingress/src/answer.rs:110`).
  Starts from main once wave 34 is merged — cited
- **Primary surface:** `crates/spindle-host` (bound-command retry and dead letter) and
  `crates/spindle-cli/src/seed.rs` (seed entry) — cited
- **Files:** `crates/spindle-host/src/host.rs`, `Host::attempt`: `Err(_) => Some(None)` at `:2747`
  (the story's `:2669-2700`, moved), retry test `:2749-2754`, `Retried::Exhausted` `:2756`,
  `lettered` `:2764-2780`, `Err` arm `:2818-2831`; the `Failed` doc `:75-78` and the
  `RefusedCommand` doc `:111-113` ("Nothing was written"); the `Host::replay`/`replayed` docs
  `:1610-1635` — cited
- **Files:** `crates/spindle-cli/src/seed.rs`: `submitted` `:1074-1113` (`:1111` maps every
  `HostError::Execute` to `Became::Rejected`), `Became` `:721-728`, `Seeding::step` `:969-1036`
  (`rejected_at` `:995-999`, request-id memory `:990`), `Recorded` `:180-185`
  (`deny_unknown_fields` `:179`), module doc `:1-30` — cited
- **Files:** `crates/spindle-ingress/src/console.rs`: `replay` `:703-777` (`result` `"failed"`
  at `:753`), `dead_letters`/`letter` `:669-701` — cited (D3, D2)
- **Files:** `crates/spindle-kernel/src/store.rs:263-280` `DeadLetter`: a serde-default field
  marking the outcome unknown (D1 A); `DeadLetter` has no `deny_unknown_fields` — cited (the
  struct), inferred (the field)
- **Files:** `crates/spindle-cli/src/serve.rs`: `did_not_run` `:1013-1036` (the function from
  `:1017`) says "did not run" for every `Failed`, plus the line fields `:841-855`; its unit-test
  builder `failed` `:1122` builds `Failed` by literal — inferred, the wording of an unknown outcome
  and a new `Failed` field
- **Files (tests):** new `crates/spindle-host/tests/spindle-host/unknown_retry.rs` plus a `mod`
  line in `crates/spindle-host/tests/spindle-host/main.rs`: a stub store wrapping `MemoryStore`
  whose `commit`/`commit_all` fail `StoreError::Unknown` (built like `supplied.rs:79-97`), on the
  `retry` fixture (`crates/spindle-host/tests/fixtures/retry`, `record-filing` `on_failure: retry`)
  through `Host::with_stores` (`host.rs:1146`) with `Retries::Retried` — inferred
- **Files (tests):** a `#[cfg(test)]` module in `crates/spindle-cli/src/seed.rs`: `spindle-cli` is a
  binary crate (`lib.rs` exports only `cli`) and the seed tests under
  `tests/spindle-cli/seed/` start the binary, which cannot be given a stub store. A supplied
  host plus `Marker::File` in a tempdir (`tempfile` is already a dependency of `spindle-cli`,
  `Cargo.toml:43`) — inferred
- **Files (tests):** `crates/spindle-ingress/tests/spindle-ingress/unknown_commit.rs` (exists,
  wave 34) gains the replay and dead-letter listing cases — inferred
- **Symbols:** `Host::attempt`, `Failed`, `DeadLetter`, `Route::answer`
  (`crates/spindle-channel/src/route.rs:307`, unchanged), `Retried::Exhausted`, `Became`,
  `Recorded`, `submitted`, `console::replay`, `console::letter`, `did_not_run` — cited
- **Contract:** `ess/domains/console.yaml:177-212` `spindle.console.ReplayResult` /
  `ReplayAnswer` and `ess/generated/jsonschema/schema/types/spindle.console.ReplayResult.schema.json`
  (D3 A) — cited (the files), inferred (the change)
- **Contract:** `ess/domains/observe.yaml` `spindle.observe.DeadLetter` (`:876-927`, D1 A/D2 A)
  and `spindle.observe.SeedState.rejected` (`:983-987`, D6), regenerated into
  `crates/spindle-observe-api/source.schema.json`, `crates/spindle-observe-api/types.rs`,
  `ui/src/api/generated/source.schema.json`, `ui/src/api/generated/types.ts` — cited (the
  files), inferred (that the decisions reach them)
- **Files (help, review round 1):** `crates/spindle-cli/src/cli.rs:536-585`, the `--seed` help of
  `run`: "rejected (it did not run: …)" `:547-548` and "a restart with the same file sends the
  rejected entries again" `:559-561` — cited. Its generated copy `website/docs/reference/cli.md:153`
  (the `--seed` row of `run`), rewritten by `task docs-generate` (`Taskfile.yml:263-266`) and
  checked by `task docs-check` (`:268-271`), never edited by hand — cited
- **Documents:** `website/docs/concepts/components-and-bindings.md:50-67` (§ "Dead letters": retry
  and dead letters `:52-57`, listing and replay `:59-67`),
  `website/docs/concepts/environment-and-admission.md:250-375` (seed counts `:255-260`, a kept
  state `:283-305`, PostgreSQL with a lease `:307-375`, whose last paragraph says a store without
  a lease records nothing, `:372-375`), `CHANGELOG.md` `[Unreleased]` `### Fixed` — cited
  (paragraphs), inferred (the edits); `docs/design/spindle.md` § 11 dead letters — inferred, only if
  it states the retry rule
- **Not touched, read:** `crates/spindle-cli/src/component.rs:354` prints `Failed` but opens no
  PostgreSQL store (no `postgres`/`with_stores` in the file); `crates/spindle-cli/src/main.rs:464`
  is the stdin run, which refuses PostgreSQL; `ExecuteError::kind` stays `"store"`
  (`crates/spindle-kernel/src/lib.rs:299`), so `spindle-telemetry`'s `Failure` and
  `spindle_command_failures_total` labels are unchanged — cited
- **Confidence:** high for `host.rs` and `seed.rs` (the story names both sites); medium for the
  contract and listing files, which depend on D1-D6
- **Would collide with:** `story:observe-label-vocabulary` on `ess/domains/observe.yaml`,
  `crates/spindle-observe-api` and `ui/src/api/generated` (under D2 A, and under D6 A for one
  summary line; D6 B adds `crates/spindle-ingress/src/observe/metrics.rs` and
  `ess/fixtures/observe/metrics/families.json`); `story:link-bound-setting` on `CHANGELOG.md`,
  `crates/spindle-cli/src/cli.rs` (another variant: U1 edits `Command::Play` `:695-751`, this
  story the `--seed` help of `run` `:536-585`) and the generated `website/docs/reference/cli.md`,
  which both regenerate — this story `depends_on` both and merges after them (review round 1);
  `story:postgres-lane-margin-ci` possibly on `crates/spindle-cli/src/serve.rs`,
  which opens the stores (`Host::with_stores`), in a function other than `did_not_run` (inferred);
  no PostgreSQL-lane test is added
- **Safety fact:** `StoreError::Unknown` reaches `Host::attempt` only as
  `Err(ExecuteError::Store(StoreError::Unknown(_)))` from a PostgreSQL commit after COMMIT was
  sent. If that arm is matched before `refusal` is computed (`host.rs:2744-2748`), every other
  `Err` keeps `Some(None)`, so today's retry and `Rejected` hold for every other store error.
  `seed.rs:179` `deny_unknown_fields` on `Recorded` is why D5 A keeps the marker format — step 2,
  unproven
- **Test time:** measured 2026-11-28: the 24 existing `retry::`, `supplied::` and
  `dead_letters::` host cases ran in 3.40 s on one thread (slot job 19941), about 0.14 s each.
  New cases, estimated: 3 host cases about 0.15 s each, 2 ingress cases about 0.2 s each,
  2 seed unit cases about 0.25 s each, 1 `serve.rs` unit case under 0.01 s; about 1.3 s added to
  the default step — measured (neighbours), inferred (new cases)
- **Checked when planned (2026-11-28, read against b546d6d2):** no file under `crates/`, `ess/` or
  `ui/` changed between 42488ced and b546d6d2. Fixed from the scoper's lines: `did_not_run` is
  `:1013-1036`, not `:1023-1038`; `spindle.observe.DeadLetter` is `:876-927`, not about
  `:895-925`; `Recorded` ends at `:185`; `tempfile` is a regular dependency of `spindle-cli`, not a
  dev-dependency — cited. Settled from the scoper's open points: the `retry` fixture's
  `filing.ledger.Record` writes a row on its outcome `recorded` (`creates: filing.ledger.Entry`,
  `domains/ledger.yaml:36-37`), and its other outcome `held` is external, so the stub's failing
  `commit` is reached once the test answers `recorded` — cited. Also generated on every
  regeneration, so shared with `story:observe-label-vocabulary` as well: `ess/generated/jsonschema`
  (`DeadLetter`, `SeedState` and `ReplayResult` schemas, `.ess-output/state.json`) and the
  `.ess-output/state.json` of `crates/spindle-observe-api` and `ui/src/api/generated` — cited
  (the files each earlier regeneration changed, `git show --name-only 60b3bb23f`). The PostgreSQL
  lane story left wave 35, so no `serve.rs` overlap remains in the wave. Still open: whether
  `Host::supplied`/`with_stores` admits a seed fixture whose component is reached over the network;
  the seed unit test may need the `notebooks` fixture or another one — inferred
