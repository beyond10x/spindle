---
format: aep.planning-md/3
id: review-result:wave32-adversary-U2-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 32 U2
relations:
- reviews: story:conform-awaits-system-steps
revision: 1
---
unit: story:conform-awaits-system-steps, head a451fac06a809e6e77b3a5d1ef9f7473efe4345f (adversary commit 0c6ca8c02 on top)
verdict: CONFIRMED
cases: executed 173→186, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 2 paths (scratch directory; a second finished mutant worktree)
needs-coordinator: (1) The one red case is the deviation the implementor documented. Decide whether to fix it, accept the case staying red, or route it out of the wave. (2) `worktree gc` keeps both mutant worktrees ("no-remote-recovery-proof"); removing them is yours.

The round-1 fix holds: the two round-1 red cases are green now. One case is red, and only through an authored scenario.

Before attacking I checked `git diff 50431a14d a451fac06 -- '*adversary_w32_u2*' '*adversary-w32-u2*'`: it is empty, so none of my round-1 files changed.

**1. Diff stat (0c6ca8c02, against a451fac06).** Only test files and fixtures.
```
 crates/spindle-conformance/tests/fixtures/adversary-w32-u2-periodic/{system.yaml,domains/hall.yaml}
 crates/spindle-conformance/tests/fixtures/adversary-w32-u2-r2/{system.yaml,domains/yard.yaml}
 crates/spindle-conformance/tests/fixtures/adversary-w32-u2-r2-scenarios/{after-nothing,two-awaits,after-generated}.yaml
 crates/spindle-conformance/tests/spindle-conformance/adversary_w32_u2.rs | 134 +
 8 files changed, 642 insertions(+)
```

**2. Cases added**, appended to `<local path>` and each run alone first:

The new fixture `adversary-w32-u2-r2` has:
- `Switch`: no bound, guarded by a minted `Boolean`.
- `Stamp`: no bound; the system mints an `Integer` code and writes it into the `Stamped` event and the row.
- `InstallTagged`: a caller command whose event carries `{generated: true}`.

| Case | Asserts | Now |
|---|---|---|
| `an_await_after_one_that_reached_nothing_mints_the_first_value` | `Switch` reaches nothing, then `Stamp` carries code 2 (tick 1), on both targets | green |
| `two_reached_awaits_mint_successive_values` | two stamps carry 2, then 3, on both | green |
| `an_await_after_a_generated_value_mints_as_the_interpreter` | after `InstallTagged`, `Stamp` carries code 3 on both | **RED** |
| `the_round_two_synthesised_suite_ends_as_on_the_interpreter` | host and interpreter give the same status for every synthesised scenario | green |
| `a_bounded_await_beside_a_periodic_binding_is_not_passed` | round-1 `busy.yaml` over the periodic fixture: the host is not Passed, and its status equals the interpreter's (Unsupported) | green |

Red output of the case run alone (the interpreter passed; the host assertion failed):
```
assertion `left == right` failed: the host: failed lamps.yard/authored/the-system-stamps-a-tagged-lamp
  failed ESS-CF-PAYLOAD — payload of lamps.yard.Stamped
    input:    lamps.yard.Stamp(code = 2.0, lamp_id = "lamp-1")
    expected: lamps.yard.Stamped.code = 3.0
    observed: code = 2.0
  … lamps.yard.Lamps holds a row where code = 3.0 … observed: {code = 2.0, lamp_id = "lamp-1", state = "Stamped"}
```

The first run of all five failed because the fixtures did not load, not because of a defect:
- the round-2 system was named `yard` instead of `lamps`;
- the host refuses a periodic binding that reads host fields ("binding tick reads fresh host fields on each occurrence").

I fixed both fixtures, then reran each case alone; the table is from that rerun.

Mutant probes, run in the separate managed worktree `spindle-w32-u2-adv-mutant-r2` at a451fac06:
- **M4:** `target.rs:915` `running.minted = minted;` replaced with `let _ = minted;` (a reached await does not keep its values). The unit's 5 `await_system_steps` tests stay green. `two_reached_awaits_mint_successive_values` goes red.
- **M5:** the periodic check at `target.rs:933` disabled (`&& false`). The unit's tests stay green. `a_bounded_await_beside_a_periodic_binding_is_not_passed` goes red.

**3. Suite runs**, after the cases existed:
- `cargo test --locked -p spindle-conformance --no-fail-fast`: exit 101 in 14 s. `141 passed; 1 failed; 2 ignored`.
- The same with `-- --skip adversary_w32_u2`: exit 0 in 16 s. `132 passed; 0 failed; 2 ignored; 10 filtered out`.
- `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast -- replay adversary_w32_u2`: exit 0. `44 passed; 0 failed` (round-1 CLI red case now green).
- The same with `-- replay --skip adversary_w32_u2`: exit 0. `41 passed`.
- `cargo clippy --locked -p spindle-conformance --all-targets -- -D warnings`: clean.
- `cargo fmt -p spindle-conformance --check`: 0.
- No added test is over 5 s.

**4. Findings** (cover 0c6ca8c02)
- **F4. Where the host's counter starts.** `crates/spindle-conformance/src/target.rs:828` (the documented deviation; the counter is used at `:882`). Category: property. Verdict CONFIRMED, origin introduced.
  - What was measured: `adversary_w32_u2.rs` `an_await_after_a_generated_value_mints_as_the_interpreter`, red as above.
  - What reaches it: an authored document (`run --scenario`, or `conform` with authored documents) that states a value the system mints, after a command that generated a value.
  - A synthesised suite does not reach it: `the_round_two_synthesised_suite_ends_as_on_the_interpreter` is green. Synthesis states every input a guard reads, and checks an awaited input against what the target reported (`synthesize/system_issued.rs`).
  - Documented by the implementor, so I rate it a note. The fix would be to advance the host's counter for each value the scenario generates; that is a deeper change than round 2's.
- F1 from round 1 is resolved: both round-1 red cases are green at a451fac06.
- F3 from round 1 is resolved: `a_bounded_await_the_host_does_not_reach_fails` makes `HostTarget::await_outcome` itself answer not-issued.

**5. Attacked and not broken**
- An await after one that reached nothing.
- Two reached awaits in one scenario.
- The periodic-binding check: the host stays Unsupported, as the interpreter does.
- Status parity of the synthesised suite with a generated value earlier in the scenario.
- Not attacked: whether a forced external refusal keeps the minted values. By reading, the interpreter commits `step.next` on a refusal too, which would match the host's write-back; no case was built.

**6. Paths written outside the worktree**
- `<local path>`: logs (`r2-*.log`, `m4.log`, `m5.log`), `round2-alone.sh`, `round2-mutants.sh`, `round2-append.rs`, `suite.sh`, `commit-msg-r2.txt`.
- `<local path>`: the mutant worktree, reverted, clean, finished.
- From round 1: `<local path>`, also finished.

```findings
- file: crates/spindle-conformance/src/target.rs
  line: 828
  category: property
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the await mint counter does not count values the scenario's earlier commands generated, so an authored document stating the interpreter's minted value (Stamped.code 3) fails on the host (2); a synthesised suite does not reach it
```
