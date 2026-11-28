---
format: aep.planning-md/3
id: review-result:wave32-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 32 U2
relations:
- reviews: story:conform-awaits-system-steps
revision: 1
---
unit: story:conform-awaits-system-steps, head bec6b515cec5ace1a1c438c6994be22516e74e41 (adversary commit 50431a14d on top)
verdict: CONFIRMED
cases: executed 172→180, red 2
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 2 paths (scratch directory; a finished mutant worktree)
needs-coordinator: the mutant worktree `spindle-w32-u2-adv-mutant` is finished but `worktree gc` keeps it ("no-remote-recovery-proof"). Removing it is yours.

**1. Diff stat (adversary commit 50431a14d, against bec6b515c).** Every path is a test file or a fixture.
```
 crates/spindle-cli/tests/spindle-cli/adversary_w32_u2.rs                 |  84
 crates/spindle-cli/tests/spindle-cli/main.rs                             |   1
 crates/spindle-conformance/tests/fixtures/adversary-w32-u2-scenarios/*.yaml (busy, dim, outage, served, wrong-crew)
 crates/spindle-conformance/tests/fixtures/adversary-w32-u2/{system.yaml,domains/hall.yaml}
 crates/spindle-conformance/tests/spindle-conformance/adversary_w32_u2.rs | 133
 crates/spindle-conformance/tests/spindle-conformance/main.rs             |   1
 11 files changed, 565 insertions(+)
```

**2. Cases added.** Each was run alone before the suite. The fixture `adversary-w32-u2` has two system commands:
- `Switch`: no `await_within`; its `bright: Boolean` input is not stated, so the system mints it; `lit` when `bright == true`, else the refusing default `dim`.
- `Dispatch`: has `await_within: PT5S`, chooses `crew` through `when_related:`, and has an `external:` refusal `outage`.

Conformance module `<local path>`:
- `an_unbounded_await_asked_again_answers_the_same`: `dim.yaml` passes on the interpreter and on the host. **RED.** The interpreter passes, the host fails:
  ```
  failed ESS-CF-AWAIT — lamps.hall.Switch not issued on "lamp-1"
    input:    lamps.hall.Switch(bright = true, lamp_id = "lamp-1")
    expected: no outcome of lamps.hall.Switch on "lamp-1" by the runner's deadline
    observed: the system issued it: outcome = lit
  failed ESS-CF-VIEW … observed: {crew = null, lamp_id = "lamp-1", state = "Lit"}
    left: Failed   right: Passed
  ```
- `a_forced_external_outcome_is_reached_past_a_refused_candidate` (`outage.yaml`): green.
- `the_free_crew_is_sent_past_a_busy_one` (`served.yaml`): green.
- `a_busy_crew_is_not_sent_within_the_bound` (`busy.yaml`): green. Covers the implementor's deviation 3 for `busy`.
- `the_synthesised_suite_ends_as_on_the_interpreter`: the host and the interpreter give the same status for every scenario, and the host skips none. Green.

CLI module `<local path>`:
- `a_replayed_unbounded_refusal_holds_as_on_the_interpreter`: `run --scenario dim.yaml` exits 0. **RED.**
  ```
  left: Some(1)  right: Some(0)
  timeline[1] lamps.hall.Switch: diverges … observed: the system issued it: outcome = lit
  ```
- `a_replayed_await_claiming_the_wrong_crew_fails_at_its_act`: exits 1 at `timeline[3]`. Green.
- `a_replayed_await_sends_the_free_crew`: green.

Mutant probes, run in a separate managed worktree; the unit's tree was not touched:
- **M1:** `target.rs:898` changed to `let reached = false` (the forced-outcome clause dropped).
- **M2:** `target.rs:461` changed to `if !not_granted {` (the forcing lapses at every candidate).
- Under both, the unit's 4 `await_system_steps` tests stayed green. `outage`, `synthesised_suite` and `dim` went red. M2's diagnostic was `expected outcome = outage / observed outcome = served`.

**3. Suite runs**, made after the cases existed:
- `cargo test --locked -p spindle-conformance --no-fail-fast`: exit 101 in 29 s. `135 passed; 1 failed; 2 ignored`.
- The same with `-- --skip adversary_w32_u2`: exit 0 in 16 s. `131 passed; 0 failed; 2 ignored; 5 filtered out`.
- `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast -- replay adversary_w32_u2`: exit 101 in 19 s. `43 passed; 1 failed`.
- The same with `-- replay --skip adversary_w32_u2`: exit 0. `41 passed; 0 failed`.
- `cargo clippy --locked -p spindle-conformance|spindle-cli --all-targets -- -D warnings`: clean.
- `cargo fmt -p spindle-conformance|spindle-cli --check`: exit 0.
- No added test is over 5 s; every test binary finished in under 9 s.

**4. Findings** (cover commit 50431a14d)
- **F1. Host mint counter.** `crates/spindle-conformance/src/target.rs:393` (`running.minted += 1` in `choices`). Category: property / parity. Verdict CONFIRMED, origin introduced.
  - The cause: every ask advances the host's mint counter, even when nothing was reached. ESS's interpreter mints from a copy of its store that it throws away, so asking again answers the same.
  - What was measured: the two red cases above.
  - What reaches it: an authored scenario (`run --scenario`, or `conform` with authored documents) whose act expects a refusing outcome of a system command that has no `await_within`, where a guard reads an input the act does not state. Synthesised suites do not reach it: they state every input a guard reads, and they drop input-decided refusals (`ESS-SYNTH-024`).
  - Same cause, implementor's deviation 2: the host counts from 1, while the interpreter counts from the scenario's own counter. So a minted value in a reached answer differs from the interpreter's once the scenario minted anything earlier.
  - Fix, named not applied: save `running.minted` before `choices` and restore it when nothing is reached.
- **F2. Forced-outcome path untested.** `target.rs:898` and `:461`. Category: mutant. Verdict CONFIRMED, origin introduced. The unit's fixture has no `external:` outcome, so M1 and M2 survive its suite. My kept tests now catch both.
- **F3. Test that cannot fail.** `crates/spindle-conformance/tests/spindle-conformance/await_system_steps.rs:447` (`an_await_answered_not_issued_fails`). Category: judgement. Verdict CONFIRMED, origin introduced. The wrapper answers `NotIssued` without asking `HostTarget`, so the test only exercises ESS's runner and cannot fail for a Spindle defect. The story's acceptance asked for exactly this test, so it is a note.
- **Deviation 1** (`crates/spindle-host/src/host.rs:1508`, `Host::identities`): no failing case. It sits outside the story's Scope, which expected no `spindle-host` change, and has no test in `spindle-host`. The gate the brief names does not cover `spindle-host`. Residue; not raised as a finding.

**5. Attacked and not broken**
- Order of `when_related:` candidates: the busy crew is tried first and the free crew is sent.
- A forced external outcome is reached past a refused candidate.
- Bounded nothing-issued on a refusal the rows decide.
- Status parity of the synthesised suite on a second fixture.
- A replayed await is still held to its claimed events.
- Not attacked: dropping the periodic-binding check (`target.rs:919-931`) is still a mutant no test catches; a fixture with a periodic binding was not built.

**6. Paths written outside the worktree**
- `<local path>`: logs, `suite.sh`, `commit-msg.txt`.
- `<local path>`: the mutant worktree, reverted, cache discarded, finished; `gc` keeps it.

```findings
- file: crates/spindle-conformance/src/target.rs
  line: 393
  category: property
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: every await ask advances HostTarget's own mint counter, so an unbounded await asked again mints a new value and reaches a branch ESS's interpreter, which answers every ask the same, never reaches; an authored expect_not_issued that passes on the interpreter fails on the host and in run --scenario
- file: crates/spindle-conformance/src/target.rs
  line: 898
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the forced-external branch of await_outcome and the no-lapse-per-candidate guard at :461 survive mutation under the unit's own tests, whose fixture has no external outcome; the kept adversary cases now catch both
- file: crates/spindle-conformance/tests/spindle-conformance/await_system_steps.rs
  line: 447
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: an_await_answered_not_issued_fails never calls HostTarget::await_outcome, since the wrapper answers NotIssued itself, so it checks ESS's runner and cannot fail for a Spindle defect
```
