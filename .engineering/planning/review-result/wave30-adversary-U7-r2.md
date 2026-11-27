---
format: aep.planning-md/3
id: review-result:wave30-adversary-U7-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 30 U7
relations:
- reviews: story:events-cursor-across-restart
revision: 1
---
```
unit: story:events-cursor-across-restart, round 2; findings cover implementation b907241f, tests at 78fe8941
verdict: CONFIRMED
cases: executed 201→214, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: the PostgreSQL lane does not run here (no SPINDLE_TEST_DATABASE_URL, and builds run on the pool's node); worktree w30-u7-adv-mutant is clean and finished, but gc keeps it until 3428a635 is on a remote ref
```

The log-id fix closes all four round-1 red cases. Two new red cases remain, both introduced: a cursor taken before a `state move` answers every moved event again, and a cursor from 0.23.0 still skips events on new memory stores.

**1. Diff stat (b907241f..HEAD)**

```
 .../tests/spindle-host/adversary_w30_u7.rs | 288 ++++++++++++++++++++-
 1 file changed, 286 insertions(+), 2 deletions(-)
```

Only my test module changed, in commit 78fe8941. The 2 deleted lines are my own `use` line and module doc line, rewritten. Between my rounds the implementor renamed and rewrote my round-1 case at `:347` in this file, on F3.

**2. New cases, run alone first** (`cargo test --locked -p spindle-host --test spindle-host adversary_w30_u7`): `11 passed; 2 failed … finished in 0.11s`

| case | b907241f | base 8eaea47a |
|---|---|---|
| `a_cursor_from_before_a_state_move_answers_no_moved_event_again` | **red** | green |
| `a_released_cursor_reads_every_event_of_a_new_memory_store_past_its_mark` | **red** | green |
| `a_sqlite_log_id_is_kept_by_its_file_and_differs_between_files` | green | does not compile there (no `events_id`) |
| `a_memory_log_id_is_kept_by_its_store_and_differs_between_stores` | green | does not compile there |
| `a_mark_taken_on_an_empty_store_reads_its_first_events_after_a_restart` (end and head cursor, beside the host's thread too) | green | red |
| `a_cursor_taken_on_a_file_a_reset_emptied_reads_on_after_a_restart` | green | green |
| the 7 round-1 cases | all green | — |

Red output:

```
adversary_w30_u7.rs:599:5 (state move)
  left: [String("w-0"), String("w-1"), String("w-2")]
 right: []
adversary_w30_u7.rs:647:9 (0.23.0 cursor), limit 1
  left: [String("t3"), String("t2"), String("t2"), String("t3"), String("t3")]
 right: [String("t1"), String("t2"), String("t1"), String("t1"), String("t3"), String("t2"), String("t2"), String("t3"), String("t3")]
```

**3. Suite run** (after the cases existed)

- `cargo test --locked -p spindle-host --no-fail-fast` exited 101: `test result: FAILED. 212 passed; 2 failed; 14 ignored; … finished in 10.20s`. The 2 failures are exactly the two red cases. The before count of 201 is 214 minus my 13 cases.
- `cargo clippy --locked -p spindle-host --all-targets -- -D warnings`: `Finished`, exit 0.
- `cargo fmt -p spindle-host --check`: exit 0, after formatting my own file with `slot --fetch … rustfmt`.

**4. Findings**

- **A cursor taken before a `state move` answers every moved event again** (`range.rs:298` with `:351`; introduced, warning, CONFIRMED).
  - **What happens:** `state move` gives the PostgreSQL copy new event ids but keeps the command origins, so the copy has a different log id. A store with a different log is read from its start with `last: None`, so every event of the copy is answered a second time.
  - **Base:** read the copy from its start and skipped by key, so it answered nothing again on a clock that moves forward.
  - **How I measured it:** a second SQLite file holding the same write at the same sequence stands in for the PostgreSQL copy. I did not run it on PostgreSQL.
  - **What reaches it:** the documented sequence `run --state`, then `state move`, then `run` on postgres, with a consumer keeping its cursor.
  - **Fix to name:** have `state move` record the source log's id in the copy, or skip by key when a store's log differs and the new run's clock is past the cursor's last sequence.
- **A 0.23.0 cursor still skips events on new memory stores** (`range.rs:300`; introduced, note, CONFIRMED).
  - **What happens:** its marks have no log id, so the head check decides. New memory stores that reached the marked positions are read after them, and their first events are skipped.
  - **Base:** handled this on a clock that moves forward.
  - **Known:** the CHANGELOG states this behaviour.
  - **What reaches it:** only a cursor held across the upgrade to this release, and only once.
  - **Fix to name:** for marks without a log id, use the base path (read from the start, skip by key).
- **The beside-the-thread read can save a mark with log 0** (`reader.rs:171`; introduced, note, INFEASIBLE).
  - **What happens:** `Reader::published_after` reads `events_id` before it reads the events. If a store's first write lands between the two reads, the mark has a position above 0 and log 0, so it falls back to the head check.
  - **What reaches it:** a race on a store's first write. I could not trigger it deterministically.
- **The wasm forwarder and the browser store have no `events_id`** (`crates/spindle-wasm/src/worker.rs:305`, the wasm `Shared` forwarder; introduced, note, INFEASIBLE).
  - **What happens:** both use the default 0, so they fall back to the head check.
  - **What reaches it:** IndexedDB positions only start over when the database is deleted. The wasm target cannot run natively here.
  - **Every native wrapper forwards the id:** Tallied, Routed, Supplied, Counted, Gated, CountingStore, SharedReading and SqliteReading.

**5. Attacked, not broken**

- **Empty store that later gets events:** a mark taken there has log 0 at position 0 and is read from the start.
- **SQLite log id:** 0 until the first event, then stable. It survives reopening the file, an in-place reset, and reads through the reading handle. Another file has another id.
- **Memory log id:** never 0, kept past an in-place reset, different for each store.
- **Reset on SQLite under `--state` before or after a restart:** works.
- **Cursor taken while a reset left the file empty:** works.
- **The 64-bit fold:** FNV-1a over the UUID text, raised to at least 1. Not attacked further: a collision is about 2^-64.
- **Earlier-release cursor on SQLite:** works.
- **PostgreSQL:** not run. Reading the code, the first visible event cannot change, because the feed hides events above an open transaction.

**6. Written outside the worktree**

- In `<local path>`: `r2-red-alone.log`, `r2-base.log`, `r2-fmt.log`, `r2-clippy.log`, `r2-suite.log`, `round2-cases.rs` and `commit-msg-r2.txt`, plus the round-1 files already reported.
- Managed worktree `<local path>`: removed.
- Managed worktree `<local path>`: still kept by gc (see `needs-coordinator`).

**7.**

```findings
- file: crates/spindle-host/src/host/range.rs
  line: 298
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a cursor taken before a state move reads the copy, another log with the same events at the same command sequences, from its start with no last key and answers every moved event again, where the base skipped them by key
- file: crates/spindle-host/src/host/range.rs
  line: 300
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a 0.23.0 cursor with no log ids, read after an upgrade onto new memory stores that reached its marked positions, skips their first events, where the base read them on a clock that moves forward
- file: crates/spindle-host/src/host/reader.rs
  line: 171
  category: concurrency
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the beside-the-thread read takes events_id before it reads the events, so a store's first write landing between the two leaves a mark at a position above 0 with log 0, which falls back to the head check
- file: crates/spindle-wasm/src/worker.rs
  line: 305
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the wasm Shared forwarder and the browser store do not implement events_id, so a browser cursor always falls back to the head check
```
