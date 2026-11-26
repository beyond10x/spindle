---
format: aep.planning-md/3
id: review-result:wave27-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U1
relations:
- reviews: story:stdin-run-fires-bindings
- reviews: story:stdin-arrange-and-line-references
revision: 1
---
```
unit: story:stdin-run-fires-bindings, story:stdin-arrange-and-line-references; findings cover head c1c60317b (unit head affd16841 plus the adversary test commit)
verdict: CONFIRMED
cases: executed 535→546, red 2 (plus 4 red outside this module, not from this unit)
origin: introduced 2 / pre-existing 1 / undecided 0
wrote-outside-worktree: 9 paths (part 6)
needs-coordinator: none
```

Two of my 11 new cases are red. Both come from this unit's diff. The bigger one: when two bindings on one event invoke commands on two different components, a stdin run and a served run don't run those commands in the same order. With `--mint-seed`, the two runs then give each row the other row's identity. The unit's own parity test only uses one component, so it can't see this. The other nine cases pass and stay as kept tests.

**1. `git --no-pager diff --stat affd16841 HEAD`**
```
 .../adversary-w27-u1/chain-in-process.yaml         | 189 ++++++
 .../fixtures/adversary-w27-u1/chain-network.yaml   | 189 ++++++
 .../tests/fixtures/adversary-w27-u1/loop.yaml      |  34 ++
 .../relay-in-process/domains/alpha.yaml            |  57 ++
 .../relay-in-process/domains/zulu.yaml             |  43 ++
 .../adversary-w27-u1/relay-in-process/system.yaml  |  53 ++
 .../relay-network/domains/alpha.yaml               |  57 ++
 .../relay-network/domains/zulu.yaml                |  43 ++
 .../adversary-w27-u1/relay-network/system.yaml     |  53 ++
 .../tests/fixtures/adversary-w27-u1/unmet.yaml     |  57 ++
 .../tests/spindle-cli/adversary_w27_u1.rs          | 652 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
 12 files changed, 1428 insertions(+)
```
Every path is a test file, a test fixture or the `mod` line. No implementation file was touched. Committed as c1c60317b through `b10x-gates bot -- commit`; the check for private names printed nothing.

**2. Cases added** in `<local path>`. All 11 ran together, filtered to this module only (`cargo test … -- adversary_w27_u1`), before the suite ran. The first run fixed fixture shapes: ESS refused `>` in a `where` condition, so the fixtures now use enum equality. I also removed a rally case that hung (part 4).

Red:
- `bindings_to_two_components_mint_alike_on_stdin_and_served` (:604). Each run is seeded with 5 and sends one `relay.alpha.Open`. Bindings `a-zulu` (to component zulu) and `b-alpha` (back to alpha) each mint an identity.
  ```
  assertion `left == right` failed: the same memo and the same copy, identities included; stdin printed {"command":"relay.alpha.Open",...}
  {"attempts":1,"caused_by":{"binding":"a-zulu","line":1},"command":"relay.zulu.AddCopy",...,"copy_id":"63033b0c-a389-435a-8097-314d939736f8"...}
  {"attempts":1,"caused_by":{"binding":"b-alpha","line":1},"command":"relay.alpha.AddMemo",...,"memo_id":"3b92d3f0-106b-4147-996e-4ec2da05b945"...}
    left: ([["3b92d3f0-…",{"case_id":"c-1"}]], [["63033b0c-…",{"case_id":"c-1"}]])
   right: ([["63033b0c-…",{"case_id":"c-1"}]], [["3b92d3f0-…",{"case_id":"c-1"}]])
  ```
- `a_specification_without_bindings_summarises_a_rejected_line_as_before` (:435). On the `ess` spec, which has no bindings, the last stderr line no longer matches the base.
  ```
    left: ["b10x-spindle: 1 rejected of 1 lines read and 0 commands their bindings invoked, so the run exits 1", ...]
   right: ["b10x-spindle: 1 rejected of 1 commands read, so the run exits 1", ...]
  ```

Green, kept:
- **Runaway:** a binding that keeps calling itself stops after `DELIVERY_LIMIT` (10,000) bound commands. Every one is printed, then one `runaway` line, and the run exits 1.
- **Undecided condition:** a `where` that reads an absent optional field prints an `unmet_binding` line with `caused_by` and `attempts: 0`, and the run exits 1.
- **Order on one component:** chained bound commands run in the order their events were emitted (AddNote, FlagCase, then TouchCase), and `caused_by.line` counts a blank line.
- **Parity on one component:** rows and events in commit order match a served run, with a seeded mint and one refused bound command.
- **Mint after an arrange line:** an arrange line in the middle of a seeded run keeps the mint's place in its sequence.
- **Arrange over a minted identity:** rejected as `invalid_arrangement`, and nothing changes.
- **Arrange row breaking an invariant:** rejected, and nothing is written.
- **Reference to an arrange line:** a reference to an arrange line's answer works, with blank lines counted.
- **Malformed references:** `$line` of -1, `"1"`, 0, the line itself, an extra member or a missing pointer each reject the line as `unresolved_reference`.

None of the new tests is over 5 s; the harness reported 0.23 s for the whole module.

**3. Suite run (after the cases existed)**
`SLOT_TIMEOUT=20m cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast` → exit 101:
```
running 546 tests
test result: FAILED. 506 passed; 6 failed; 34 ignored; 0 measured; 0 filtered out; finished in 30.33s
```
- 2 failures are my red cases.
- 3 are the known `ui/dist` cases: `deploy::the_ui_is_served_from_the_binary` and two `observe_panel` cases.
- 1 is `adversary_processes::a_peer_slow_to_start_is_reached_within_the_connect_bound` (the play-component peer path, "the channel is broken: closed"). That code is outside this unit's diff; I did not investigate it.

The before count of 535 is this binary's total minus my 11, as the module run reported ("535 filtered out").

Clippy (`-p spindle-cli --all-targets -D warnings`) exits 0 after one lint fix in my file. `rustfmt --check` on my file exits 0.

**4. Findings**
- **NEEDS-CHANGE, introduced. Commands bound to two components run in a different order on stdin and served.** `crates/spindle-cli/src/lines.rs:339-346`: the stdin run takes the events from one queue and runs each event's routes in binding-name order. A served run (`spindle-host/src/host.rs` `settle`) runs everything waiting on one component-to-component link before the next link, in sorted link order. With bindings to two components, the bound commands run in a different order; with a seeded mint, the rows get swapped identities, so the acceptance claim "same rows" fails.
  - What reaches it: a consumer's simulation that uses `--mint-seed` on a spec with more than one component.
  - Possible fix: run stdin bound commands per link in the served run's order, or document the difference.
- **CONFIRMED, introduced, note. Stderr summary changed for specs with no bindings.** `crates/spindle-cli/src/main.rs:825` changed the summary line ("commands read" became "lines read and N commands their bindings invoked") even when the spec has no bindings. The implementor declared this deviation; no doc or test quoted the old wording.
  - What reaches it: any consumer that parses that line.
- **INFEASIBLE, pre-existing, note. The rally page's "Drive it by hand" example never reads stdin.** `examples/rally/ess` has `rally-table` with `reached_by: network`, so on the laptop environment `run` serves HTTP and never reads stdin. Probe: `timeout 20` exited 124 after printing only `{"components":["rally-table"],"listening":…}`. The base has the same reach; the unit's edit at `website/docs/examples/rally.md:58-59` now claims stdin bindings fire on an example that never takes the stdin path.
  - What reaches it: anyone following the page.
  - I removed the case rather than keep a test that hangs.

**5. Attacked and could not break**
- D4: refused and kernel-rejected bound commands, and the exit codes.
- Runaway and unmet bound commands.
- Breadth-first order and parity on one component.
- Retries and `--drop-retries` (read against the served `attempt` code).
- Reference edge cases: forward, self, 0, negative, string, extra member, missing pointer, pointing at an arrange line, blank lines.
- Arrange lines: rows that ESS's checks refuse, invariants, identities that collide with minted ones, arranging after commands, the seeded sequence after an arrange line.
- D2 refusal of `escalate` (matches the router).
- The implied component.

**6. Paths written outside the worktree** (all under `<local path>`):
`run1.log`, `run2.log`, `run3.log`, `suite.log`, `clippy.log`, `clippy2.log`, `m.rs`, `rally-probe.sh`, `commit.txt`. The probe also wrote `requests.jsonl`, `rally.out`, `rally.err` and `rally-state/` in the same directory. Inside the worktree's ignored `target/debug`, the pool fetched `b10x-spindle` and the test binary.

**7.**
```findings
- file: crates/spindle-cli/src/lines.rs
  line: 339
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: bound commands to two components run in binding-name order on stdin but per link on a served run, so a seeded stdin run and a seeded served run give the rows swapped identities (adversary_w27_u1::bindings_to_two_components_mint_alike_on_stdin_and_served)
- file: crates/spindle-cli/src/main.rs
  line: 825
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the stderr summary of a rejected line changed wording for specifications without bindings too (adversary_w27_u1::a_specification_without_bindings_summarises_a_rejected_line_as_before)
- file: website/docs/examples/rally.md
  line: 58
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: pre-existing
  message: the page's stdin example serves HTTP because rally-table is reached_by network, so it never reads stdin, yet the unit's edit now says stdin bindings fire there
```
