---
format: aep.planning-md/3
id: review-result:wave29-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 29 U1
relations:
- reviews: story:binding-failure-escalates
revision: 1
---
```
unit: story:binding-failure-escalates (wave 29 U1), findings cover head 1f06b18d; adversary tests committed on top as 3083ce3c
verdict: NEEDS-CHANGE
cases: executed 685→694, red 6 (3 new, 3 UI cases already red on the base)
origin: introduced 3 / pre-existing 1 / undecided 0
wrote-outside-worktree: 1 path
needs-coordinator: decide whether `conform_runs_an_escalating_binding` gets HostTarget::observe_invocations (code in host.rs and target.rs, see finding 1) or a narrower acceptance text
```

## 1. Files I changed (`git diff --stat 1f06b18d..HEAD`)
```
 crates/spindle-cli/tests/spindle-cli/adversary_w29_u1.rs           | 386 +++
 crates/spindle-cli/tests/spindle-cli/main.rs                       |   1 +
 crates/spindle-conformance/tests/spindle-conformance/adversary_w29_u1.rs | 375 +++
 crates/spindle-conformance/tests/spindle-conformance/main.rs       |   1 +
```
All four are test files. I changed no implementation file.

## 2. New cases

**`crates/spindle-cli/tests/spindle-cli/adversary_w29_u1.rs`.** I ran this module on its own first. Three cases are red and one is green.

- `conform_skips_no_escalating_scenario_as_unsupported` is **red**. Output from that first run:
  ```
  "ship-on-order/binding/refusal/no-address: status \"unsupported\", unsupported [\"`ship-on-order` invokes `depot.shipping.Ship` exactly 1 time(s)\" (\"ESS-CF-INVOCATION\")]",
  "ship-on-order/binding/refusal/oversize: status \"unsupported\", unsupported [... same ...]",
  ```
- `a_served_run_does_not_log_an_escalated_attempt_as_dropped` is **red**. The fixture uses a universal escalation, and the decisions file does not answer `Ship`. The event history shows `OrderPlaced, OrderFlagged`, so the escalation did run. The log still says it was dropped:
  ```
  "b10x-spindle: shipping did not run depot.shipping.Ship (ship-on-order), dropped: external.depot.shipping.Ship.cancelled gave no answer: ..."
  ```
- `a_served_run_reports_an_unmet_escalation` is **red**. `ShipmentStuck` has a required `carrier` field that the input lacks, and `Ship` is refused with `no-address`. Nothing is published, which is correct, but the served run's stderr is `""`, so the unmet escalation is reported nowhere.
- `a_stdin_line_reports_an_unmet_escalation` is **green**. On stdin the line carries `escalated.{event, unmet}` with no payload, nothing fires, the run exits 1 and stderr names `unmet`.

**`crates/spindle-conformance/tests/spindle-conformance/adversary_w29_u1.rs`.** All 5 cases are green. They compare the host with ESS 0.99.1's interpreter, including `PortAnswer::Untyped` (a failure with no declared outcome):
- a universal escalation of an attempt that fails with no declared outcome;
- a per-refusal table whose fallback retries, with `[untyped, no-address]` and `[untyped, untyped]`;
- `escalate {outcomes:[no-address]}` with `drop {except:[no-address]}`;
- a table whose fallback is `escalate {except:[cancelled]}`;
- a host built with `Retries::Dropped`: `busy` makes 1 attempt and does not escalate; `oversize` escalates and `flag-stuck` runs once.

Two of these variants failed on their first run because ESS refused the fixture: a table must name its fallback with `except:`. That was a mistake in my fixtures, not a finding. I fixed both, and all 5 then passed.

## 3. Suite runs (after the cases existed)
- `cargo test --locked -p spindle-conformance --no-fail-fast`: exit 0, `79 passed; 0 failed; 1 ignored`.
- `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast`: exit 101, `609 passed; 6 failed; 33 ignored; finished in 30.22s`. The 6 failures are my 3 red cases plus `deploy::the_ui_is_served_from_the_binary`, `observe_panel::the_panel_is_one_self_contained_page` and `observe_panel::the_panel_may_be_framed`, which are already red on the base.
- The "before" count is these runs minus my 5 + 4 cases. Clippy `-D warnings` on both crates is clean, and fmt `--check` exits 0.

## 4. Findings and your four deviations

1. **Deviation 1, conform (acceptance).** The only check left unsupported in both escalating scenarios is `ESS-CF-INVOCATION` ("invokes Ship exactly 1 time"). Every check on `ShipmentStuck` passes.
   - **What is missing:** `HostTarget` does not implement `ConformanceTarget::observe_invocations` (`target.rs:430`). The ESS default answers unsupported, and the base has the same gap (`git show 97dedd3b:…/target.rs` has no `observe_invocations`).
   - **Size of the change:**
     - `Report` has to carry each attempt's binding, command and input. Today `Ran`, `RefusedCommand` and `Failed` hold an attempt count but no input. This is about 15 lines in `Host::attempt` / `host.rs`, plus the export.
     - `Running` gains a list of invocations, and `observe_invocations` filters it by binding and command, with the input converted to ESS values by the existing `node()`. This is about 35 lines in `target.rs`.
     - Side effect: every binding scenario in every fixture would start checking its invocations. That includes `binding_where.rs`, which today tolerates these checks being unsupported. New red cases outside escalation could appear, so this is its own story.
   - **Documents that say otherwise:** `CHANGELOG.md:19` says these scenarios "now execute", which is false (finding 4).
2. **Deviation 2.** `HostTarget` answers unsupported for an unmet escalation, as D4 decided. I confirmed that ESS fills a placeholder from its type witness there (`interpret/bindings.rs:420-446`). No finding.
3. **Deviation 3, served stderr (introduced).** The served loop reads only `report.failed` (`serve.rs:610`), so neither a published nor an unmet escalation (`host.rs:2316`) gets a log line or an observe record. The red case covers the unmet case. On stdin the unmet escalation is reported.
4. **Escalated attempt logged as "dropped" (introduced).** Under a universal escalation, or a table whose fallback escalates, an attempt that fails with no declared outcome is escalated. `serve.rs:622` still logs it as `dropped`. The line is older than this unit, but on the base an escalating binding was refused at start, so it could not appear for one.
5. **Deviation 4, periodic bindings.** The router skips periodic bindings, so a periodic `on_failure: escalate` neither escalates nor is checked at start against what its component publishes. On the base it was not refused either, and ESS's interpreter has no periodic escalation path to compare with. I left this as a note and wrote no test.

## 5. What I attacked and could not break
- **Parity with ESS:** the event is published once, its payload comes from the input, and the binding that listens for it runs once. This holds for universal, per-refusal, no-declared-outcome and fallback-escalates policies.
- **`--drop-retries`:** escalations are kept on the host and on stdin.
- **Refusal at start:** an escalation into an event the component does not publish is refused, on stdin and served. The specification with no components gets an implied component that publishes every event, so it is not refused by mistake.
- **`Route::escalate`:** it returns `None` for drop and retry answers, and an exhausted retry does not escalate.
- **Untested mutant:** deleting `route.on_failure = policy.fallback.word()` in `dropping_retries` would only show on a failed network send under `--drop-retries`. I found no cheap way to test that.

## 6. Paths written outside the worktree
- `<local path>`. It holds `conf-alone.log`, `cli-alone.log`, `conf-suite.log`, `cli-suite.log` and `commit-msg.txt`.
- The tests write temporary copies of the fixture under the build node's `CARGO_TARGET_TMPDIR`.

```findings
- file: crates/spindle-conformance/src/target.rs
  line: 430
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: pre-existing
  message: HostTarget does not implement observe_invocations, so conform still answers both escalating scenarios as unsupported (ESS-CF-INVOCATION), against the acceptance that none is skipped as unsupported
- file: CHANGELOG.md
  line: 19
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the changelog says conform's scenarios that select an escalated refusal now execute, while both still end unsupported
- file: crates/spindle-cli/src/serve.rs
  line: 622
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a served run logs an attempt that failed with no declared outcome as dropped even when its binding escalated it and the escalation event reached its listener
- file: crates/spindle-cli/src/serve.rs
  line: 610
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a served run reads only report.failed, so an unmet escalation (and a published one) is reported nowhere on stderr or in the observe record
```
