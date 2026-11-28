---
format: aep.planning-md/3
id: story:seed-under-a-lease
kind: story
status: implemented
title: A run with a lease seeds once whatever token it took
relations:
- serves: vision:spindle
- depends_on: story:leader-exits-promptly-on-sigterm
scope:
- confidence: cited
  path: AGENTS.md
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/database.rs
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs
- confidence: inferred
  path: crates/spindle-kernel/src/postgres.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/postgres.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: cited
  path: website/docs/reference/cli.md
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T04:16:54Z", actor: "human:operator", revision: 5, decided_on: {"recorded":{"review_outcome":11}}}
- {from: "proposed", to: "active", at: "2026-11-28T04:16:54Z", actor: "human:operator", revision: 6, decided_on: {"recorded":{"review_outcome":11}}}
- {from: "active", to: "implemented", at: "2026-11-28T08:49:47Z", actor: "human:operator", revision: 7, decided_on: {"recorded":{"test_result":2,"review_outcome":14}}}
---
## Outcome

A run on PostgreSQL with `database_lease` set decides its seed from a record of the seed's progress
kept in the component's tables, whatever lease token it took, as `seed.json` keeps it for a
`--state` directory: with no record and no rows it submits the seed from entry 1; with a record of
the same file part done it resumes after the count; once one process has completed it, it skips;
with no record and rows it writes a complete record and skips (D5 B). This replaces D5 A of
`story:several-writers-one-postgres` (D2 A).

The record is a row `seed` in the `<prefix>_p_lease` read model of the first component in name
order, holding `{"digest", "done", "rejected"}` as `seed.json`'s `Recorded` does (D4 A); the lease
thread writes it on the lease connection only while the lease row names this process's token
(D3 A). A store that holds rows and no record (seeded under 0.26.0, or filled by `state move`)
gets a complete record for the file and skips the seed with one line; a store with no rows seeds
from entry 1 (D5 B). A run without `database_lease` records nothing and sends the whole seed on
every start, as in 0.26.0 (D1 A). Resets write and delete the record as they do `seed.json` on a
`--state` run (D6 A). Another file exits 1 before the run serves a request (D7 A).

## Why

Found by wave 31's U6 adversary: under D5 A of `story:several-writers-one-postgres` the seed runs
only while every lease is at its first token, so with `database_lease` set a restart of the only process, a changed seed file in a later
release, and a first process that died before seeding (exit 3 on a misfit after taking its leases)
never seed again. That narrows `story:seed-at-start`'s "on every start" for runs that set the
lease.

## Acceptance

Each test below is a PostgreSQL-lane test (`#[ignore]`, run by `task check:postgres`) with
`database_lease` set, each under the 60 s bound, in `spindle-cli` `postgres/replicas.rs` except
`a_deposed_process_cannot_write_the_seed_record`.

- `a_seed_after_a_lease_taken_without_seeding_runs`: the test takes token 1 through
  `spindle_kernel::Lease::take` and releases it, as a process that exited 3 on a misfit leaves it
  (as `adversary_w31_u6.rs:854-864` does). The next run leads at token 2, prints
  `ended: N sent, N applied`, and each seeded row is read once.
- `a_restart_after_a_completed_seed_skips_it`: a restart with the same file prints the "already
  ran … no entry is sent" line naming the store rather than a path, prints no `ended:` line, and
  `/__spindle/ready` answers `200` within 10 s.
- `another_seed_file_is_refused_before_serving`: on a store with rows and a complete record of one
  file, a restart with another file exits 1 before it serves a request, naming both digests, prints
  no `ended:` line, and leaves the rows and the record unchanged (D7 A).
- `another_seed_file_on_an_empty_store_seeds_from_the_first_entry`: on a store with no row and a
  record of one file, a restart with another file sends every entry from the first, prints
  `ended: N sent, N applied`, and the record then holds the new file's digest (D2 A).
- `a_standby_that_takes_over_after_the_seed_skips_it`: after the leader completed the seed and is
  killed, the standby leads and skips the seed on the record, with the line that names the store,
  not a token.
- `a_killed_leader_resumes_the_seed_after_its_count`: a leader killed (SIGKILL) after entry 1 of 3
  leaves a record at count 1, and the next process sends only entries 2 and 3.
- `a_store_seeded_without_a_record_skips_and_records`: a store with rows and no record (as 0.26.0
  leaves it) skips the seed with one line and writes a complete record; the next start with the
  same file skips on that record (D5 B).
- `a_reset_of_a_leased_run_rewrites_the_record`: after `POST /api/reset` with no world, the `seed`
  row reads `done` 0 before the first entry is sent and the full count once the seed has ended;
  after `POST /api/reset` with a world, it reads every entry done and no entry is sent (D6 A).
- `a_reset_by_a_run_without_a_seed_deletes_the_record`: a leased run started without `--seed` on a
  store with a record answers `POST /api/reset`, and the `seed` row is gone after it (D6 A).
- `a_deposed_process_cannot_write_the_seed_record` (`spindle-kernel` `postgres.rs`): handle A holds
  the lease at token 1, handle B takes it at token 2, and A's write of the `seed` row is refused by
  name with the row unchanged (D3 A).
- The three tests of `adversary_w31_u6.rs` that check D5 A (`the_seed_runs_only_under_the_first_token`
  `:549-595`, `a_seed_with_one_lease_taken_before_is_skipped_naming_it` `:842-880`,
  `a_restart_after_a_clean_stop_skips_the_seed` `:882-914`) are each rewritten to this story's rule
  or retired, and the PR names which.
- `docs/design/spindle.md:436`, `website/docs/concepts/environment-and-admission.md:313-318` and
  `:584`, `README.md:89-97`, the `--seed` help (`cli.rs:549-555`, generated `cli.md`) and
  `AGENTS.md:40` state the record instead of D5 A; the 0.26.0 entry of `CHANGELOG.md` stays, the
  change goes under `[Unreleased]`.
- The unit's closing record names the time of one `task check:postgres` run before and after this
  story's tests and the `pg_stat_activity` connection peak of the run after, sampled as
  `story:postgres-lane-runs-locally` does; when the peak comes within 50 of the `max_connections`
  in `scripts/postgres-lane.sh`, the same change raises that value in the script and in
  `.github/workflows/ci.yml`, and the options check of `task check` stays green.

## Decided when scoped

Taken 2026-11-28 on the scoper's default, each reversible. D5's default is B; the others' is A.

- **D1 Without `database_lease`.**
  A (default, taken): unchanged; no record, and every start sends the whole seed.
  B: record on every PostgreSQL run (two writers with no lease would race on it).
- **D2 The rule it replaces.**
  A (default, taken): D5 A of `story:several-writers-one-postgres` is dropped. A leading process at
  any token decides from the store's record, as `begin` does from `seed.json`: no record means seed
  from entry 1; the same file part done means resume after the count, rejected entries resent; the
  same file all done prints one line and sends nothing; another file exits 1 naming both digests
  unless `Host::rows` sums to 0.
  B: keep D5 A and add the record only for token 1.
- **D3 Who writes the record.**
  A (default, taken): the lease thread, on the lease connection, in one statement that writes only
  while the lease row names this process's token and session. `Kept`'s channel carries the request
  and the main thread waits for the answer, bounded by the lease connection's `statement_timeout`
  (term/3, `postgres.rs:472`). No new connection, so the `database_connections` check is unchanged.
  B: a second connection of its own, which raises `POSTGRES_RESERVED` from 1 to 2: hosted budgets
  that pass today may fail at start.
  C: a projection row written inside each entry's own append transaction: exactly once, but a
  refused entry appends nothing, an entry may write to several components, and it needs eventlog
  API that `spindle-kernel` does not use yet.
- **D4 Where the record lives.**
  A (default, taken): a row `seed` in the `<prefix>_p_lease` read model of the first component in
  name order, holding `{"digest", "done", "rejected"}` as `Recorded` does. The table already exists
  from 0.26.0's `store migrate`, so no migration is needed, and the fingerprint stays
  `spindle-lease/1`.
  B: a new read model `<prefix>_p_seed` created by `store migrate`; a hosted database then needs
  this release's migrate before the run starts.
  C: the same row in every component's lease table.
- **D5 A store with rows and no record** (seeded under 0.26.0's D5 A, or filled by `state move`,
  which ignores `seed.json`).
  A: seed as a `--state` run without `seed.json` does, then write the record; an entry that is not
  `existing_instance:` may be rejected, and the run then exits 1.
  B (default, taken): write the record as complete and skip the seed with one line; with no rows,
  seed from entry 1. Changes no file of `## Scope` against A.
  C: scan the log for events whose `Provenance::Seed` digest is this file's: precise, but it reads
  the whole log into memory (`postgres.rs:700-744`).
- **D6 Resets on a leased PostgreSQL run.**
  A (default, taken): as on `--state`. A reset with no world writes count 0 before its first entry;
  one with a world writes every entry done; one by a run without a seed deletes the record (the
  branch at `serve.rs:763` extended to the store).
- **D7 Another file once a standby leads.**
  A (default, taken): exit 1 before it serves any request (as a standby it has only answered
  `503`), naming both digests. It is restarted into the same exit until the file or the store
  changes, as on `--state`.
  B: skip with a warning and serve.

## Scope

Derived 2026-11-28 by `story-scoper`, against `plan/wave-32` (main at 0.26.0). Every line is
**cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).
Assumes the decisions taken below (D5 B, the rest A).

- **Primary surface:** `crates/spindle-cli` (the seed and its start-time decision) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:594-625`: the D5 A branch `taken_again` that skips the seed at any token above 1, which this story replaces; `:357-381`: the leases are taken before the stores open, so a misfit exits 3 after the leases were taken (`:376`); `:749-777`: the reset branches, where only a `--state` run removes the record (`seed::forget`, `:763`) — cited
- **Files:** `crates/spindle-cli/src/seed.rs`: `begin` `:240-366` (takes `state: Option<&Path>` and records nothing without it, `:247-262`), `MARKER` `:160`, `Recorded` `:165-172`, `recorded` `:178`, `record` `:197`, `forget` `:215`, `Seeding::{new, mark, marked, loaded, restart}` `:489-610`, field `marker: Option<PathBuf>` `:464`; module doc `:23-32` ("A store kept elsewhere (PostgreSQL, memory) records nothing") — cited
- **Files:** `crates/spindle-cli/src/database.rs`: `Kept::taken_again` `:537-544` (removed, or kept only for the log line); `Leases::keep` `:391-411`, whose stop channel `mpsc::channel::<()>` `:398` would carry the record's reads and writes under D3 A; `Leases::renewing` `:416-480` — cited
- **Files:** `crates/spindle-kernel/src/postgres.rs`: `Lease` `:400-642` gains a fenced read and write of a `seed` row next to `LEASE_ROW` `:256` in the read model `LEASE` `:247`, with statements next to `TAKE`/`RENEW`/`RELEASE` `:675-698` — inferred (D3 A, D4 A)
- **Symbols:** `spindle_cli::seed::{begin, Seeding, Recorded, MARKER, forget}`, `database::{Kept, Leases}`, `Kept::taken_again`, `spindle_kernel::Lease`, `LEASE_ROW`, `Provenance::Seed` (`seed.rs:831`) — cited
- **Tests:** `crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs:549-595` (`the_seed_runs_only_under_the_first_token`), `:842-880` (`a_seed_with_one_lease_taken_before_is_skipped_naming_it`, which this story reverses), `:882-914` (`a_restart_after_a_clean_stop_skips_the_seed`): all three check D5 A's line "is not submitted … at token" — cited
- **Also likely:** new `#[ignore]` PostgreSQL-lane tests in `crates/spindle-cli/tests/spindle-cli/postgres/replicas.rs` — inferred
- **Documents:** `README.md:89-97`; `website/docs/concepts/environment-and-admission.md:305-318`, `:584`; `docs/design/spindle.md:436` (states D5 A); `crates/spindle-cli/src/cli.rs:549-555` (the `--seed` help, which generates `website/docs/reference/cli.md:153`); `CHANGELOG.md` `[Unreleased]`; `AGENTS.md:40` (the `seed.json` wording) — cited
- **Confidence:** high for the files: the story names D5 A and `seed.json`, and both are read at the lines above. Medium for the kernel surface: it rests on D3 A and D4 A
- **Would collide with:** any unit on the PostgreSQL lease surface of `spindle-cli` (`database.rs` `Leases`/`Kept` and the lease thread), the seed start and reset paths of `serve.rs`, `seed.rs`, the `Lease` type in `spindle-kernel/src/postgres.rs`, and the PostgreSQL tests in `adversary_w31_u6.rs` and `postgres/replicas.rs`
- **Safety fact:** no existing statement reads or writes a row of `<prefix>_p_lease` other than `lease`: `Fence::check` reads only `LEASE_ROW` (`postgres.rs:301`), `TAKE`/`RENEW`/`RELEASE` are keyed by it (`:538-541`), and `Leased::apply` writes nothing (`:271-277`). A `seed` row there therefore changes no fence, take or renewal. Pointed at `file:line` (level 2), unproven. Also unproven: that a reset (`Host::empty`, `crates/spindle-host/src/host.rs:1922`) and eventlog's handling of the `spindle-lease/1` fingerprint never delete that table's rows. The `lease` row surviving a reset already depends on this.
