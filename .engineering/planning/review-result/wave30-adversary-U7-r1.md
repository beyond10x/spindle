---
format: aep.planning-md/3
id: review-result:wave30-adversary-U7-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U7
relations:
- reviews: story:events-cursor-across-restart
revision: 1
---
```
unit: story:events-cursor-across-restart, head 71c2b1f1 (findings cover implementation 3428a635)
verdict: CONFIRMED
cases: executed 200→207, red 5
origin: introduced 3 / pre-existing 2 / undecided 0
wrote-outside-worktree: 6 paths (part 6)
needs-coordinator: worktree w30-u7-adv-mutant is clean and finished, but gc keeps it until 3428a635 is on a remote ref; remove it once the branch is pushed
```

**1. Diff stat (3428a635..HEAD)**

```
 .../tests/spindle-host/adversary_w30_u7.rs         | 372 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 2 files changed, 374 insertions(+)
```

Only test files changed, in commit 71c2b1f1. After the red run I changed one doc comment and the formatting of one call. Neither changes what a case does.

**2. Cases added** (`crates/spindle-host/tests/spindle-host/adversary_w30_u7.rs`)

Run alone on head: `cargo test --locked -p spindle-host --test spindle-host adversary_w30_u7` gave `2 passed; 5 failed … finished in 0.09s`. Each case takes under 0.1 s.

| case | head | base 8eaea47a | `>=`→`>` mutant |
|---|---|---|---|
| `a_new_memory_store_past_the_mark_is_read_from_its_start` (clock moves forward) | **red** | green | red |
| `…_on_a_fixed_clock` | **red** | red | red |
| `a_cursor_from_before_a_reset_and_a_restart_reads_every_event_after_the_reset` (SQLite) | **red** | green | red |
| `a_cursor_from_another_run_reads_every_event_after_a_reset_in_this_run` (SQLite) | **red** | green | red |
| `a_cursor_at_the_end_answers_the_same_cursor_after_a_restart` | **red** | red | red |
| `an_idle_cursor_from_before_a_restart_answers_nothing_on_a_fixed_clock` (end and head cursor, limits 0/1/100) | green | red | red |
| `a_cursor_from_two_runs_back_reads_both_runs_events_on_a_fixed_clock` | green | red | green |

The four event-skipping cases fail with the same output:

```
assertion `left == right` failed: limit 1
  left: [String("t3"), String("t2"), String("t2"), String("t3"), String("t3")]
 right: [String("t1"), String("t2"), String("t1"), String("t1"), String("t3"), String("t2"), String("t2"), String("t3"), String("t3")]
```

The memory case on a forward clock fails at `adversary_w30_u7.rs:162` with `the end cursor of the first host, limit 1`. The case at `adversary_w30_u7.rs:347` fails like this:

```
assertion `left == right` failed: at the end, the answer is the same cursor
```

**3. Suite run** (after the cases existed)

- `cargo test --locked -p spindle-host --no-fail-fast` exited 101: `test result: FAILED. 202 passed; 5 failed; 14 ignored; … finished in 10.27s`. The 5 failures are exactly my red cases. The before count of 200 is 207 minus my 7 cases.
- `cargo clippy --locked -p spindle-host --all-targets -- -D warnings`: `Finished`, exit 0.
- `cargo fmt -p spindle-host --check`: exit 0.
- Mutant (`head(component)? >= mark.position` changed to `>` at `range.rs:271`, in a separate managed tree), `events_range` only: `11 passed; 1 failed`. The unit's own `a_cursor_resumes_after_a_restart` catches it, so this mutant does not survive.

**4. Findings**

- **The position check cannot tell a new store from the old one** (`range.rs:271`, introduced, blocker). A cursor from another run reads a store from the marked position on whenever `head >= mark.position`. A new memory store, or a SQLite file a reset recreated, that has reached that position is taken for the old store, and its first events are skipped without an error.
  - **Your D2 question:** proven, not refuted. The unit's acceptance-2 test passes only because its second host writes less than the first did.
  - **What the story requires:** Acceptance 2 asks for "every event of the second host", and D2 B says to read from the start a store "that holds no write at the mark's position". The head check is the weaker form, and the head fails Acceptance 2 read literally.
  - **ESS:** requires nothing here. The 0.99.2 conformance crate has no events cursor, and the story claims no ESS parity.
  - **Regression:** on a clock that moves forward, the base passes all three of these cases, because it read another run's store from its start and skipped by key.
  - **What reaches it:**
    - memory store: `spindle run` with a memory store restarted, then the same or a longer scenario replayed before the client polls again;
    - SQLite: `POST /api/reset` under `--state`, before or after a restart.
  - **Fix to name:** store identity can't come from position alone. Each store needs an id of its own, kept in the mark (for example a random id written when a SQLite file is created, and one per memory store opened). That needs a kernel change beyond `range.rs`; the alternative is to check that the write at the mark carries the mark's sequence and ordinal.
- **The docs promise more than the code does** (`host.rs` doc comments of `published_after` and `published_head`, CHANGELOG entry; introduced, warning). They say a later run reads "after each store's head". That does not hold in the cases above.
- **"The same cursor" in the CLI reference** (`website/docs/reference/cli.md:107`, pre-existing, note). An idle cursor at the end, read after a restart, answers a cursor of the new run, not the same cursor. Same at base. The doc of `Page::next` allows it ("every store it marks is the one it was taken from"); the CLI reference does not. Fix one of the two texts, or return `after` unchanged when nothing was answered.
- **Acceptance 2 on a fixed clock** (`range.rs:271`, pre-existing, blocker for this story). The same skip also happens at base, but this story exists to fix it.

**5. Attacked, not broken**

- SQLite feed positions with no event at them: `feed_end` and `committed_after` use the same `global_seq`.
- Several components with different heads: the check is made per component.
- A cursor from two restarts back, on SQLite.
- The head cursor on SQLite.
- Limits 0 and 1: a limit-0 read returns a cursor that resumes correctly.
- A write answered in part (the Part and Within paths): no underflow at `limit - rest.len()`.
- The mutant `>=`→`>`: the unit's tests catch it.

**6. Written outside the worktree**

- `<local path>` holds `red-alone.log`, `base.log`, `mutant.log`, `mutant-unit-suite.log`, `suite.log` and `commit-msg.txt`.
- Managed worktree `<local path>`: removed.
- Managed worktree `<local path>`: reverted to clean and finished, but gc keeps it (see `needs-coordinator`).

**7.**

```findings
- file: crates/spindle-host/src/host/range.rs
  line: 271
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: '"head >= mark.position" takes a new memory store or a reset-recreated SQLite file that reached the mark for the old store, so a cursor from another run skips its first events, where the base read them on a forward clock'
- file: crates/spindle-host/src/host.rs
  line: 1379
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the published_after/published_head docs and the CHANGELOG entry promise that a later run reads after each store's position, which fails for a new or reset store that reached that position
- file: crates/spindle-host/src/host/range.rs
  line: 271
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: acceptance 2 read literally fails on a fixed clock once the second memory host writes as much as the first; the base fails it too
- file: website/docs/reference/cli.md
  line: 107
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: an idle cursor at the end read after a restart answers a cursor of the new run, not "the same cursor" the CLI reference promises
```
