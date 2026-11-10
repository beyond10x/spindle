---
format: aep.planning-md/3
id: review-result:w17-u7-adversary-1
kind: review-result
status: active
title: U7 binding delivery, adversary pass 1
relations:
- reviews: story:binding-delivery-specified
revision: 1
---
unit: U7 story:binding-delivery-specified, tree spindle-w17-delivery at 92c33990 (unit/w17-delivery) plus my uncommitted test files
verdict: NEEDS-CHANGE
cases: executed 29→32, red 1
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 8 paths
needs-coordinator: none

**1. Diff.** `crates/spindle-ingress/tests/spindle-ingress/main.rs` +1 (`mod adversary_delivery;`);
new untracked `crates/spindle-ingress/tests/fixtures/adversary-unmet/` (system.yaml,
domains/{left,right,notices}.yaml; "relay v1 — 4 file(s), valid") and
`crates/spindle-ingress/tests/spindle-ingress/adversary_delivery.rs`. Test files only.

**2. Cases added**
- `adversary_an_unmet_input_of_a_request_that_ran_away_is_listed`: RED. `Loop` publishes `Looped`;
  binding `thank-looped` has an unmet input and `ping` starts an echo loop that never stops. Exit
  101: `the record lists no unmet input of a request that ran away: {…"status":501,"unmet":[]}`,
  expected `[{"binding":"thank-looped","command":"relay.notices.Thank","component":"notices","reason":null}]`;
  `test result: FAILED. 2 passed; 1 failed`.
- `adversary_an_unmet_input_two_bindings_down_is_listed`: green (`Send` → `relay` → `Relay` →
  `thank-relayed`).
- `adversary_a_delivery_that_failed_after_an_attempt_is_not_unmet`: green; a second `SendThanks`
  fails with `attempts >= 1` and both records have `unmet == []`.

**3. Suite runs.** `cargo test -p spindle-ingress --test spindle-ingress`: `31 passed; 1 failed`,
exit 101; with `--skip adversary_delivery::` `29 passed`, exit 0. `cargo test -p spindle-control
--lib`: 5 passed. `cargo test -p spindle-cli --test spindle-cli observe::`: 28 passed. `pnpm test`:
53 passed. `pnpm typecheck`: exit 0.

**4. Findings** (92c33990)
1. `crates/spindle-ingress/src/answer.rs:267` (with `router.rs:244`), NEEDS-CHANGE, introduced: `run`
   maps `Err(HostError::Runaway)` to `(answer, Vec::new(), …)`, discarding the report, so `unmet` is
   `[]` though the unmet input was collected at `host.rs:1114` before `settle` ran away. Reached by a
   spec whose bindings trigger each other plus an unmet input on the same request; rare. Fix: carry
   the partial report's failures on `HostError::Runaway`, or read them before answering.
2. `crates/spindle-ingress/src/observe.rs:258`, CONFIRMED, introduced: dropping `if failed.attempts
   == 0` from `UnmetInput::of` leaves the unit's suite green (read, not run); the third case pins it.
3. `ess/domains/observe.yaml:666`, CONFIRMED, introduced: the `DeliveryGuarantee` summary says
   Spindle redelivers an overdue envelope; no runtime path calls `Sender::redeliver` and the
   conformance target answers redelivery unsupported (`crates/spindle-conformance/src/target.rs:423`).

**5. Attacked and not broken.** Only `Delivered` failures have `attempts: 0` (`host.rs:1497`);
`attempts` per policy matches `Route::retries` (`route.rs:249`); guarantee spelling matches ESS
serde; nothing reads an old request log; Rust, TypeScript, schema and fixtures agree; unmet two hops
down is listed.

**6. Outside the worktree.** `…/w17/delivery/adv/` (cases.log, suite.sh, suite logs, ui logs).

```findings
- file: crates/spindle-ingress/src/answer.rs
  line: 267
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a request whose bindings run away drops the host report, so its record answers unmet [] although an unmet binding input was collected before the runaway, contradicting RequestRecord.unmet's "every event binding whose input was unmet"
- file: crates/spindle-ingress/src/observe.rs
  line: 258
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no test the unit added produces a delivered failure with attempts >= 1, so removing the attempts == 0 guard of UnmetInput::of leaves the unit's suite green; adversary_a_delivery_that_failed_after_an_attempt_is_not_unmet now pins it
- file: ess/domains/observe.yaml
  line: 666
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the published DeliveryGuarantee summary says Spindle redelivers an unacknowledged at_least_once envelope, but no runtime path calls Sender::redeliver and the conformance target answers redelivery unsupported
```
