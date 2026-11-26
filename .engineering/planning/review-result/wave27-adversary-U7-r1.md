---
format: aep.planning-md/3
id: review-result:wave27-adversary-U7-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U7
relations:
- reviews: story:refused-bound-command-recorded
revision: 1
---
unit: story:refused-bound-command-recorded (U7), findings cover head bafb1a1c1 (unit head f77a92b83 plus my test commit)
verdict: REFUTED: no case went red; 3 note-level judgement findings
cases: executed 143→150, red 0
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 paths, all in the assigned scratch directory (part 6)
needs-coordinator: none

I could not break the unit. All 7 cases I added pass, and the generated observe files show no drift.

**1. Diff stat (f77a92b83..HEAD).** Only test files and fixtures. No non-test path.
```
 .../fixtures/adversary-w27-u7/domains/audit.yaml   | 118 ++++++
 .../fixtures/adversary-w27-u7/domains/balance.yaml |  61 +++
 .../fixtures/adversary-w27-u7/domains/till.yaml    |  39 ++
 .../tests/fixtures/adversary-w27-u7/system.yaml    |  78 ++++
 .../tests/spindle-ingress/adversary_w27_u7.rs      | 455 +++++++++++++++++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
```
Commit bafb1a1c1 on `unit/w27-refused-bound`. The privacy grep printed nothing.

**2. Cases added** in `<local path>`. The new fixture `tests/fixtures/adversary-w27-u7` has:
- `Post` publishes once, and two bindings carry it to `Apply` and `Record`.
- `Apply` publishes `Applied`, which a retrying binding carries to `Review`.

All 7 cases are green now:
- `the_refused_bound_document_is_what_a_run_records`: the unit's own `ess/fixtures/observe/requests/refused-bound.json` equals a live run of the unit's `refused-bound` fixture (amounts 10, 0, 5000), with `at` and `duration_us` left out. Status 202, `caused_by`, `kind` and the order all match.
- `both_envelopes_of_one_publish_name_their_cause`: one publish refused twice; both entries say `caused_by: ledger.till.Post`.
  - None of the unit's tests has two envelopes from one publish.
  - So a mutant that keeps the cause only for the first id of a publish (`host.rs:1852-1857`) would pass the unit's tests. This case would catch it. I did not run it against a mutated copy.
- `a_refusal_down_a_chain_names_the_bound_command_that_published`:
  - The entry names `caused_by: ledger.balance.Apply`, with `attempts: 5` (`RETRY_ATTEMPTS`).
  - The metric reads 1, not 5.
  - `/api/events` has no `Review`, and no error field value leaks into the record.
- `a_dropped_retry_is_listed_after_one_attempt`: the `--drop-retries` mode (`Retries::Dropped`) gives `attempts: 1`.
- `outcome_refusals_are_listed_before_kernel_rejections`: one request with both kinds lists the outcome refusal first, then the `kind` entry, as `RequestRecord.refused_bound` says.
- `an_unmet_input_names_the_command_whose_event_it_could_not_fill` (host report, `adversary-unmet`):
  - An unmet input two hops down names `relay.right.Relay`, with `kind: None`.
  - In the runaway, `failed` names `relay.left.Loop`.
  - After the runaway, the next call's causes are still correct, so `discard` clears the map.
- `a_refusal_stays_in_its_own_request`: three posts give list lengths `[2, 0, 2]`, and the metrics count each entry (2 and 2).

The first run of the module had 5 cases red, but only because my own fixture did not compile (ESS refused it with `[empty_change]` on two outcomes). That was a fixture mistake, not a finding. I fixed the fixture, and on the rerun all 7 passed in 0.32 s. No test is over 5 s.

**3. Suite runs, after the cases existed.**
- `cargo test --locked -p spindle-ingress --no-fail-fast`: exit 0. Unit tests 21 passed; integration 129 passed in 10.55 s.
- Same with `-- --skip adversary_w27_u7` (my module left out): exit 0. 21 passed and 122 passed, which is the 143 baseline.
- `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-ingress -- --check`: exit 0, after formatting my file only with `slot --fetch`.
- `slot task check:the_generated_types_have_no_drift check:every_fixture_answer_validates_against_its_schema`: exit 0.
  - "101 artifact(s), current in ess/generated/jsonschema"
  - "49 model type(s), current" for Rust and for TypeScript
  - "5 instance(s), 5 valid, root spindle.observe.RequestLog.document"

**4. Judgement findings**

| file:line | verdict | origin | finding | what reaches it |
|---|---|---|---|---|
| `crates/spindle-ingress/src/observe.rs:386` | CONFIRMED | introduced | The metric's `refusal` label mixes outcome names and kernel kinds. An outcome named like a kernel kind (for example `store` or `broken_invariant`) counts in the same series as a kernel rejection. The record itself keeps the two apart. | A model with such an outcome name; none found in the repository. |
| `crates/spindle-host/src/host.rs:1852` | CONFIRMED | introduced | No test fails if the `self.remote.is_none()` guard is removed. Without it, a split-run host would add one `staged_by` entry per envelope sent to a peer and never remove it. | Every `play --component` / `deploy` process; only if the guard is lost later. |
| `crates/spindle-host/tests/spindle-host/refusal.rs:240` | CONFIRMED | introduced | `a_refusal_from_another_process_has_no_cause` only checks that something is absent. It would pass if `caused_by` were always `None`. spindle-host has no test where `caused_by` is set; only the spindle-ingress tests check that. | Running the spindle-host suite alone. |

**5. Attacked and could not break**
- The cause when one publish stages several envelopes, and down a chain of bindings.
- Retries in both modes, and the count of attempts.
- Order: outcome refusals first, then kernel rejections.
- Clearing on discard and after a runaway. I found no leak in an in-process run: every staged id is removed when invoked or cleared.
- Kernel rejections before a runaway are listed; outcome refusals before it are not, as documented.
- `caused_by` is left out rather than written as `null`.
- The metric's labels and counting once per request.
- No error field value in the record.
- `/api/events` has no refusal entry.
- The log stays `spindle-requests/1` (checked through the fixture comparison).
- The unit's request-log fixture matches a live run.
- The 52 regenerated schema files match a fresh generation.
- The split-run cause is `None`: it comes only from the unit's own test, and finding 3 applies to it.

**6. Paths written outside the worktree**, all under `<local path>`: `first-run.log`, `second-run.log`, `third-run.log`, `drift.log`, `suite.log`, `suite-skip.log`, `clippy.log`, `commit-msg.txt`, and an empty `fix.sed`.

**7. Findings block**
```findings
- file: crates/spindle-ingress/src/observe.rs
  line: 386
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the refusal label of spindle_bound_refusals_total puts outcome names and kernel error kinds in one value space, so an outcome named like a kernel kind shares the kernel rejection's series
- file: crates/spindle-host/src/host.rs
  line: 1852
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: no test fails when the remote.is_none() guard is removed, which would leak one staged_by entry per envelope sent to a peer in a split run
- file: crates/spindle-host/tests/spindle-host/refusal.rs
  line: 240
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the host's only caused_by test checks absence alone and would pass if caused_by were always None; spindle-host has no test where caused_by is set
```
