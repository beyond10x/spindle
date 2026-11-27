---
format: aep.planning-md/3
id: review-result:wave30-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U4
relations:
- reviews: story:look-ahead-asks-only-where-it-matters
revision: 1
---
unit: wave 30 U4, story:look-ahead-asks-only-where-it-matters. The findings cover head d673a353, with my tests committed on top as 6f31f03c.
verdict: CONFIRMED (one note-level finding; the two red cases are INFEASIBLE)
cases: executed 439→444, red 2
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: remove the two probe worktrees I created (part 6)

The unit's main change holds. Every request the interpreter answers the same under every forcing is answered with no decider, and the kernel agrees with ESS 0.99.2 in every case I ran. Two cases are red, both caused by the unit and both on a command shape nothing in this repo uses. The walk that asks no decider now reads past an external branch even when the decider will take that branch. So a row set read only there makes the commit conflict when another write lands in between, and a store error on that read is returned. A third finding: one line the unit added can never run.

**1. diff --stat (d673a353..HEAD)**
```
 .../tests/spindle-kernel/adversary_w30_u4.rs       | 676 +++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   1 +
```
Both are test paths. No implementation file was touched.

**2. Cases added** in `<local path>`. Each command is added to a scratch copy of the `adversary-look-ahead` fixture.

| Case | What it checks | Now | Base (8eaea47a) |
|---|---|---|---|
| `every_order_of_an_external_move_beside_an_external_refusal_agrees` | `LendBoth`, every declaration order ESS compiles (30): two external branches whose look-ahead answers differ, plus an input guard over an `Optional` input. Unforced and forced to each external branch, against the interpreter; where ESS answers alike under every forcing, the kernel answers with no decider. | green | red (the defect this unit fixes) |
| `every_order_over_a_stored_reference_agrees` | `ReturnBoth`, 8 orders, stored reference: an external move, an external refusal, an external branch narrowed by `when_subject`; 7 books including one with no home and one nobody holds. | green | green |
| `a_row_set_after_an_external_move_agrees` | `LendBusy` (external move, then a branch guarded by a row set) against the interpreter. | green | green |
| `a_taken_external_move_is_not_refused_by_a_row_set_it_never_read` | The decider takes `approved`; a desk is opened between the kernel's scan of the desks and its commit. | **red** | green |
| `a_failing_read_past_a_taken_external_move_is_not_reached` | The decider takes `approved`; the scan of the desks fails. | **red** | green |

The first run of the new cases alone, before any suite run:
```
cargo test --locked -p spindle-kernel --test spindle-kernel adversary_w30_u4 -- --test-threads=4
---- ..._a_failing_read_past_a_taken_external_move_is_not_reached stdout ----
panicked at crates/spindle-kernel/tests/spindle-kernel/adversary_w30_u4.rs:671:5:
assertion `left == right` failed: a read of the desks after `approved` answers the request (scans: ["probe.ahead.Desk"])
  left: Err("store: the desks cannot be read")
 right: Ok("approved")
---- ..._a_taken_external_move_is_not_refused_by_a_row_set_it_never_read stdout ----
panicked at crates/spindle-kernel/tests/spindle-kernel/adversary_w30_u4.rs:658:5:
assertion `left == right` failed: a desk opened beside the loan refuses `approved` (scans: ["probe.ahead.Desk"])
  left: Err("probe.ahead.Desk * is at version Some(4), not Some(3)")
 right: Ok("approved")
test result: FAILED. 3 passed; 2 failed; 475 filtered out; finished in 3.65s
```
The module takes 3.65 s; no case is over 5 s. The origins come from the same module run in a base worktree: 4 passed, and only the `LendBoth` case failed there.

**3. Suite run** (after the cases existed): `cargo test --locked -p spindle-kernel --no-fail-fast` exited 101.
```
unittests: test result: ok. 15 passed; 0 failed
main.rs:   test result: FAILED. 427 passed; 2 failed; 51 ignored; finished in 10.03s
doctests:  test result: ok. 0 passed
```
The 2 failures are my red cases. "Before" is 444 minus my 5 cases. clippy `-D warnings` and fmt `--check` both exit 0.

Mutants, each in a separate mutant worktree, never in the unit's tree:
- **M1:** the equality check reduced to "all answers none" (the old rule applied everywhere). Caught by 2 cases of `adversary_look_ahead` and by my `LendBoth` case.
- **M2:** the new `unknown_instance:` → wrong-state mapping in `ahead_answer` disabled (`if false`). Not caught: the full kernel suite stays green apart from my 2 red cases, which fail anyway.
- **M3:** the fallback to the asking walk on an undecidable guard disabled. Caught by 2 cases of `adversary_look_ahead` and by my `LendBoth` case.

**4. Findings** (all three on execute.rs at d673a353)

- **F1, execute.rs:1567-1568** (the walk that asks no decider). INFEASIBLE, warning, introduced.
  - Measured: adversary_w30_u4.rs:658. The scan of a row set past `approved` is added to the commit's preconditions even though the decider takes `approved` first. A concurrent desk write then refuses the loan with a conflict. ESS, and the base, stop at `approved` and read no row set.
  - Reach: nothing found. No spec in `ess/` or `examples/` puts a row set after an external branch. The shape is valid ESS and the kernel claims to run both constructs.
  - Possible fix: drop the scans this walk recorded when its answer does not stand.
- **F2, execute.rs:1583-1584** (the implementor's open point). INFEASIBLE, note, introduced.
  - Measured: adversary_w30_u4.rs:671. A store error raised past an external branch is returned. Before, the decider taking that branch meant the read never happened.
  - ESS answers `approved` for this request; my `LendBusy` agreement case confirms it on a working store.
  - Reach: only a store that fails, such as a backend error on Postgres. The other error the implementor named, `Unsupported` from a guard over the subject, is not reachable: an absent row puts nothing in `passed`, and a held-state guard on a command that addresses no instance does not compile.
  - Possible fix: fall back to the asking walk on any error once `passed` is non-empty.
- **F3, execute.rs:1691.** CONFIRMED, note, introduced.
  - The `unknown_instance:` → wrong-state mapping in `ahead_answer` can never run. `ahead_answer` only receives branches the look-ahead selected, which needs their condition to read true, or external branches. The `unknown_instance:` condition always reads false. M2 survives the full suite.
  - Possible fix: remove the line, or point its doc at `unknown_instance`.

**5. Attacked and not broken**
- The equality rule against ESS: an external move beside an external refusal, every order, with a missing, a closed and an open desk, and the `Optional` input left out and given.
- The stored-reference look-ahead with three external branches, including one narrowed by `when_subject`.
- The fallback to the asking walk on an undecidable guard (M3 is caught).
- Removing `missing_reference`: the plain no-decider walk now covers everything it decided.
- `decide_admission_not_provided_asks_no_decider`: it runs the command and asserts both the outcome and that nothing was asked.

**6. Paths written outside the worktree**
- Scratch logs and notes in `<local path>`: `run1.log`, `base1.log`, `m1.log`, `m2.log`, `m3.log`, `suite.log`, `clippy.log`, `commit-msg.txt`.
- `<local path>` (`worktree` CLI, at 8eaea47a): holds an uncommitted copy of my test module and its `mod` line.
- `<local path>` (`worktree` CLI, at d673a353): mutant reverted; holds the same uncommitted copy. Both have test builds fetched into their `target/`.

No builds ran from `~/.cache`.

**7.**
```findings
- file: crates/spindle-kernel/src/execute.rs
  line: 1568
  category: concurrency
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: the walk that asks no decider scans a row set past an external branch the decider then takes, so that branch's commit is refused as a conflict by a write to an entity it never read (adversary_w30_u4.rs:658)
- file: crates/spindle-kernel/src/execute.rs
  line: 1584
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: a store error raised past an external branch is now returned where the decider taking that branch, as ESS and the base do, answers the request (adversary_w30_u4.rs:671)
- file: crates/spindle-kernel/src/execute.rs
  line: 1691
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the unknown_instance-to-wrong-state mapping added to ahead_answer can never run, since look-ahead never selects an unknown_instance branch, and disabling it leaves the full kernel suite green
```
