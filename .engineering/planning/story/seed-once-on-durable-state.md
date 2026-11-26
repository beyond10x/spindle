---
format: aep.planning-md/3
id: story:seed-once-on-durable-state
kind: story
status: draft
title: A seed is applied once to a durable state
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/seed.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed/durable.rs
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: website/data/status.json
- confidence: cited
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 16
---
# A seed is applied once to a durable state

## Outcome

`run --seed` on a durable `--state` applies the seed once. A restart on the same state does not send
the seed's entries again, so an entry that updates, moves or removes a row is not applied a second
time. Today the restart replays every entry: creations are counted `already-there`, but every other
entry runs again and changes the rows a second time.

Decided when drafted: the run records in the state that the seed ran, with the digest of the seed
file. A restart with the same digest skips the seed and prints one line saying so; a restart with a
different digest exits 1 before listening, naming both digests, unless the state holds no row. A
reset with no world still applies the seed again (story:reset-reapplies-seed).

Reported 2026-11-26 by a consumer that keeps its state across restarts.

## Acceptance

- `a_restart_does_not_apply_the_seed_again`: a seed that creates a row and then updates it, run on a
  state, stopped and run again on that state: the row shows one update, and the second start prints
  that the seed already ran.
- `a_changed_seed_on_a_seeded_state_stops_the_run`: a restart with an edited seed file exits 1 before
  listening, naming both digests.
- `a_reset_applies_the_seed_again_after_a_restart`: after the skipped seed, a reset with no world
  applies it from its first entry.

## Scope

Derived 2026-11-26 by `story-scoper`, against `plan/wave-27` (wave 26 merged). Every line is
**cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli`: the seed module, and the serve start sequence and reset path — cited
- **Files:** `crates/spindle-cli/src/seed.rs`: `Seed` `:61-76` (gains the file digest, from the text `load` already reads at `:87`); `Seeding` `:206-220`; `new` `:234`; `cut` `:259`; `restart` `:275`; `finish` `:292`; `end` `:304`; `step` `:340-382` (the marker is written after each entry); module doc `:17-20` ("Spindle keeps no record of request ids across starts") — cited
- **Files:** `crates/spindle-cli/src/serve.rs`: the `--state` directory and lock `:271-277`; host opened `:379-391`; listener bound `:400`; `Seeding::new` on every start `:408-415`, where the marker is read, the seed skipped or resumed, or a changed digest refused before `:400`; reset with a seed `:451-464`; `reset_seeded` `:494-516`; `Serving.seed` doc `:115-118` — cited
- **Files:** `crates/spindle-cli/src/cli.rs:316-337`: the `--seed` help, which says at `:333-334` "Request ids are not kept across starts … every other entry runs again" — cited
- **Symbols:** `Seed`, `Seeding::{new, step, end, cut, restart, finish, pending}`, `serve::serve`, `Serving::state`, `Host::rows` (`crates/spindle-host/src/host.rs:1390`, for "the state holds no row"), `spindle_kernel::private::open_or_create` (`crates/spindle-kernel/src/private.rs:43`, never truncates, so a rewrite does `set_len(0)` and rewind as `state_lock.rs:120-123` does), `ess_conformance::known_failures::sha256` (the ESS 0.99.1 checkout, `crates/verify/ess-conformance/src/known_failures.rs:44`, `&[u8] -> "sha256:<hex>"`) — cited
- **Files (tests):** `crates/spindle-cli/tests/spindle-cli/seed.rs:394-450` `a_second_run_on_one_state_applies_nothing_new` asserts that the second start sends 3 entries; it is rewritten to expect the skip line and `counted == [0,0,0,0]`. The same file gets one `mod` line at `:1224-1226` — cited
- **Also likely:** a new test module `crates/spindle-cli/tests/spindle-cli/seed/durable.rs`, so that this story's tests stay out of `seed.rs`'s body. It reuses `serve`, `scratch`, `write`, `environment`, `grant`, `counted`, `reset` and `holding_desk` from `seed.rs` — inferred
- **Not needed:** a new dependency. `spindle-cli` already depends on `ess-conformance` (`crates/spindle-cli/Cargo.toml:19`), which exports `sha256`, so `Cargo.toml` and `Cargo.lock` stay as they are — cited
- **Documents:** `README.md:66-72`; `website/docs/concepts/environment-and-admission.md:231-245` ("What a seed does not do"); `crates/spindle-docs/src/status.rs:436-441` (the "Seed at start" row: its text, and its test path `:441` if the test is renamed); `CHANGELOG.md` `[Unreleased]` — cited. Regenerated from those: `website/docs/reference/cli.md:136`, `website/docs/status.md:70`, `website/data/status.json:373` — inferred, by the generator that produces them
- **Not touched:** `crates/spindle-host`, `crates/spindle-kernel`, `crates/spindle-ingress` (only `Host::rows`, `private::open_or_create` and `console::reset` are called, as they are); `crates/spindle-cli/src/main.rs` (`seed::load` at `:600` keeps its signature and the digest lives inside `Seed`); `state.rs` and `state_lock.rs` — inferred
- **Must keep (story:reset-reapplies-seed):** `serve.rs:451-453` runs `reset_seeded` only `if let Some(seeding)`. A start that skips the seed must still build a `Seeding` with nothing pending, or acceptance 3 fails. Keep the tests `seed.rs:1016`, `:1094`, `:1141`, `:1183`, `seed/adversary_w25_reseed.rs`, `seed/adversary_w25_reseed_r2.rs` and `seed/rejected.rs` green. None of them pass `--state`, so with the marker written only under `--state` they write none — cited
- **Confidence:** high for the files, because the story's own scope names them and each line was checked against this tree. Medium for the marker's behaviour around a reset that carries a world and around a crash; see D1 and D2.
- **Would collide with:** any unit that touches `crates/spindle-cli/src/serve.rs` between the start sequence and the serve loop (`:400-481`) or `reset_seeded` `:494-516`. In this wave that is story:serve-loop-signal-exit-answers: it edits the loop's signal check, `seeding.step` and `listener.next` (`:427-450`), 12 lines below this story's `:408-415`, and probably adds reset-and-seed tests to `tests/spindle-cli/seed.rs`. Any unit on `seed.rs` (`Seeding`), on the `run --seed` help in `cli.rs`, on the "Seed at start" status row, or on regenerated `cli.md` / `status.md` / `status.json`.
- **Safety fact:** `seed.json` beside `spindle.lock` survives a reset and is ignored by `state move`. A reset moves aside or removes only each `<component>.sqlite` (`host.rs:871`, `:2166-2197`), and `state move` checks and opens only `<component>.sqlite` by name (`state.rs:48`, `:65`). Proven to step 2 (`file:line`), unproven.

Decided when scoped (unchanged): `seed.json` in `--state`, `0600`, holding the digest and the number of entries done, written after each entry. Same digest: resume from the count, or print one line and send nothing when every entry is done. Different digest: exit 1 before listening, naming both digests, unless `Host::rows` sums to 0. Rows and no marker (0.21.0 or earlier): seed as today, then write the marker. Not covered: PostgreSQL (no `--state`).

Tests to add (each starts the binary at most twice on one sqlite state, with no network and no model call; none timed yet, so name any test over 5 s; 60 s per test, 120 s for the step):
- `a_restart_does_not_apply_the_seed_again` (acceptance 1)
- `a_changed_seed_on_a_seeded_state_stops_the_run` (acceptance 2: exit 1, both `sha256:` digests on stderr, no listening line)
- `a_reset_applies_the_seed_again_after_a_restart` (acceptance 3)
- `a_run_stopped_mid_seed_resumes_after_the_last_entry_done` (decided: resume; holds an entry with `holding_desk`, then SIGTERM)
- `a_changed_seed_on_a_state_with_no_row_runs_from_its_first_entry` (decided: the empty-state exception)
- `a_state_with_rows_and_no_marker_is_seeded_then_marked` (decided: an older state)
- `the_seed_marker_is_private` (mode `0600`, Unix)
- `a_restart_after_a_reset_with_a_world_sends_no_seed_entry` (D1, under the default)

## Decided when scoped

- The marker is a file `seed.json` in the `--state` directory beside `spindle.lock`, mode 0600
  through `spindle_kernel::private`, holding the seed file's digest and the number of entries done.
  It is written after each entry, so a run stopped mid-seed resumes after the last entry done.
- On start with the same digest the run resumes from the recorded count; with every entry done it
  prints one line and sends nothing. A different digest exits 1 naming both digests, unless the
  state holds no row (`Host::rows`), when the seed runs from its first entry.
- A state that holds rows and no marker (written by 0.21.0 or earlier) gets the seed applied as
  today, then the marker.
- A start that skips the seed still builds a `Seeding` with nothing pending, so a later reset with no
  world seeds again (story:reset-reapplies-seed).
- Not covered: a PostgreSQL store (no `--state` directory); a restart there replays as today.
