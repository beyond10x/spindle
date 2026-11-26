---
format: aep.planning-md/3
id: story:seed-once-on-durable-state
kind: story
status: draft
title: A seed is applied once to a durable state
relations:
- serves: vision:spindle
revision: 3
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

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli`, the seed module and the start of the serve loop — cited
- **Files:** `crates/spindle-cli/src/serve.rs:263-269` (`--state` directory created and locked by `spindle.lock`), `:367-383` (host opened), `:392` (listener bound), `:400-407` (`Seeding::new` on every start, no state consulted), `:425-427` (one entry stepped per loop turn) — cited
- **Files:** `crates/spindle-cli/src/seed.rs:203-235` (`Seeding`, `submitted` starts empty), `:319-329` (request-id already-there, this start only), `:191-199` + `:366-397` (`existing`/`submitted`: already-there only for an `existing_instance:` outcome), `:290-306` (`end`, where a "seed ran" marker would be written) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:300-318` (`--seed` help: "Request ids are not kept across starts … every other entry runs again", :312-314) — cited
- **Symbols:** `Seeding::new`, `Seeding::step`, `Seeding::end`, `Seeding::pending`, `serve::serve`, `Serving::state`, `Host::rows` (`crates/spindle-host/src/host.rs:1344`, for "the state holds no row") — cited
- **Files:** `crates/spindle-cli/tests/spindle-cli/seed.rs:398` `a_second_run_on_one_state_applies_nothing_new` asserts the second start sends 3 entries ("3 sent, 0 applied, 3 already there") and has to be rewritten; `crates/spindle-docs/src/status.rs:432-434` names that test and states the old rule — cited
- **Also likely:** new acceptance tests in `crates/spindle-cli/tests/spindle-cli/seed.rs` — inferred; `crates/spindle-cli/Cargo.toml` + `Cargo.lock` for a digest crate (no workspace crate depends on `sha2`/`blake3` directly; `sha2` is only transitive, `Cargo.lock:1991`, `:2002`) — inferred
- **Documents:** `README.md:59-61`, `website/docs/concepts/environment-and-admission.md:223-233` ("What a seed does not do"), `crates/spindle-docs/src/status.rs:432` — cited; generated `website/docs/reference/cli.md:134`, `website/docs/status.md`, `website/data/status.json`, `CHANGELOG.md` `[Unreleased]` — inferred
- **Not touched:** `crates/spindle-kernel` and `crates/spindle-host` if the marker is a file in the state directory (uses only the existing `Host::rows`); `crates/spindle-ingress` (`console::reset` stays as it is) — inferred
- **Must keep (wave 25, story:reset-reapplies-seed):** `serve.rs:434-446` sends a reset to `reset_seeded` only `if let Some(seeding)`, so a start that skips the seed must still build a `Seeding` with nothing pending, or a later reset with no world will not seed again (acceptance 3). Also keep `Seeding::cut` `:246`, `restart` `:262` (`next = 0`, `submitted.clear()`), `finish` `:278`, `reset_seeded` `serve.rs:477-499`, the tests `seed.rs:1015`, `:1093`, `:1140`, `:1182` and `tests/spindle-cli/seed/adversary_w25_reseed.rs`, `adversary_w25_reseed_r2.rs`. None of these tests use `--state` or a restart — cited
- **Confidence:** medium. The gap and the surfaces are read from the tree. Where the marker lives and when it is written are my recommendation; the story does not decide them.
- **Would collide with:** any unit touching `spindle-cli`'s `serve.rs` (`Serving` struct `:71-119`, start sequence `:367-427`), `seed.rs` (`Seeding`), `main.rs:569-593` (building `Serving`) or the `run` arguments and `--seed` help in `cli.rs`. That covers story:run-fails-on-rejected-line: a rejected seed entry exits 1 before listening, which moves the seed ahead of `serve.rs:392` and changes `Seeding::step`/`end`. It also covers story:clock-and-mint-in-every-mode: `SystemClock` at `serve.rs:370`, new `run` flags in `cli.rs`, new `Serving` fields. Also the generated `cli.md` and the status row "Seed at start".
- **Safety fact:** a marker file in the state directory survives a reset and is ignored by `state move`. A reset moves aside only each `<component>.sqlite` (`host.rs:865`, `Host::empty` `:1254-1280`), and `state move` opens only `<component>.sqlite` by name (`state.rs:48`, `:65`). Reached by step 2 (`file:line`); unproven.

Gap confirmed: yes, cited. Every start builds a fresh `Seeding` (`serve.rs:400-407`) with no request ids remembered (`seed.rs:223-235`, module doc `:14-17`). An entry counts as already-there only when its command answers an `existing_instance:` outcome (`seed.rs:381-392`) or repeats a request id from this start (`:319-320`). So an update, move or remove entry is submitted again and applied again on every restart. `cli.rs:312-314` and `README.md:59-61` say this in so many words. No marker exists anywhere: `serve.rs` reads nothing from the state directory except the lock.

Where the marker lives: a file in the `--state` directory, next to `spindle.lock` (`state_lock.rs:17`), for example `seed.json` holding `{"file", "digest"}`. Create it `0600` through `spindle_kernel::private` (`private.rs:43`) and write it from the CLI when the seed's end line prints (`Seeding::end`, `seed.rs:290`). On start, read it after the host opens and before `Listener::bind_reading_on` (`serve.rs:383-392`):
- Same digest: print one line and build a `Seeding` with nothing pending.
- Different digest: exit 1 naming both digests, unless `Host::rows` sums to 0.

Do not put it in the store. A marker stream like `RESET` (`sqlite.rs:82-85`) would mean a new `Store` method (`store.rs:207`) in the sqlite, postgres and memory stores and in the host wrappers (`host.rs:440`, `:510`, `host/rows.rs:375`). It would sit in one component's file among several, and a reset would wipe it with that file (`host.rs:1275`). Open (inferred):
- A run stopped mid-seed writes no marker, so its restart replays the whole seed. Write the marker at the start, or keep progress per entry?
- A state that holds rows but has no marker (from 0.21.0) gets the seed applied as today, then the marker.
- On postgres there is no `--state` (`serve.rs:278-282`) and the state outlives the run, so a restart there still replays. The story does not cover this.

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
