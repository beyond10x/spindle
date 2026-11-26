---
format: aep.planning-md/3
id: review-result:wave27-adversary-U8-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U8
relations:
- reviews: story:cross-component-related-reads
revision: 1
---
```
unit: story:cross-component-related-reads, worktree spindle-w27-cross-reads, findings cover 0708ac37c (my tests are commit 67ec28cbc)
verdict: NEEDS-CHANGE
cases: executed 178→183, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 5 paths (part 6)
needs-coordinator: none
```

I found one real bug: when the meter itself creates a hold, its own guard answers "charged" where ESS's interpreter answers "on-hold". The second red case is a race between two processes that I built on purpose, and I could not show that anything reaches it.

**1. Diff stat (`git --no-pager diff --stat 0708ac37c..HEAD`).** Only test files and fixtures changed; no implementation file was touched.
```
 .../quantifiers/domains/holds.yaml                 |  84 +++++
 .../quantifiers/domains/usage.yaml                 | 159 ++++++++
 .../adversary-w27-u8/quantifiers/system.yaml       |  42 +++
 .../adversary-w27-u8/subject/domains/holds.yaml    |  41 ++
 .../adversary-w27-u8/subject/domains/usage.yaml    |  73 ++++
 .../fixtures/adversary-w27-u8/subject/system.yaml  |  34 ++
 .../tests/spindle-host/adversary_w27_u8.rs         | 420 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
```

**2. Cases added** in `crates/spindle-host/tests/spindle-host/adversary_w27_u8.rs`, with fixtures under `crates/spindle-host/tests/fixtures/adversary-w27-u8/`. The first run was these cases alone (`cargo test --locked -p spindle-host --test spindle-host -- adversary_w27_u8`), before any suite run. Every test is under 0.1 s.

- **Red:** `a_hold_the_meter_creates_is_the_one_its_guard_reads`
  - Fixture `subject`: `meter` accepts `metering.usage.HoldOrg`, which `creates: metering.holds.Hold`. That entity belongs to `holds`. ESS 0.99.1 compiles this fixture.
  - Output:
    ```
    panicked at crates/spindle-host/tests/spindle-host/adversary_w27_u8.rs:280:5:
    assertion `left == right` failed: the host answers the interpreter's outcomes
      left: ["held", "charged"]
     right: ["held", "on-hold"]
    ```
- **Red:** `a_hold_placed_between_the_check_and_the_commit_refuses_the_charge`
  - Uses supplied hooked stores. A second hold lands in the holds' store after `Routed::unmoved` checks it and before the meter's store commits.
  - Output:
    ```
    panicked at crates/spindle-host/tests/spindle-host/adversary_w27_u8.rs:407:5:
    assertion `left == right` failed: a charge decided over holds that moved before it committed is kept (answered charged)
      left: 1
     right: 0
    ```
- **Green, kept:** `count_forall_and_absence_over_another_components_rows_agree_in_memory` and `…_on_sqlite`.
  - 18 steps on fixture `quantifiers`, checked against `ess_conformance::interpret`: `count: {gte: 2}`, `forall:` (including true over no rows, when the owner has none), and `exists: false` over another component's holds.
  - Holds are placed, levelled and released between the charges.
  - It also covers an outcome that only publishes (the `commit_events` path) and a subject update beside an own-entity `affects:` (the `commit_all` path), each guarded by a scan of the owner's store. If the host stopped removing those foreign scans before the meter's store checks its own, the meter's store would refuse these writes as a conflict and this case would go red. The unit's own tests do not cover that.
- **Green, kept:** `a_split_meter_refuses_each_quantifier_by_name`. On SQLite, a host running the meter alone refuses all three guards by name and keeps no charge.

**3. Suite run** (after the cases existed): `cargo test --locked -p spindle-host --no-fail-fast` gave `180 passed; 2 failed; 13 ignored` plus 1 unit test, exit 101, wall time 16 s. The two failures are the two red cases above. With `--skip adversary_w27_u8` it gave `177 passed; 13 ignored` plus 1. `cargo clippy -p spindle-host --all-targets -D warnings` is clean and rustfmt is clean on the new files.

**4. Findings**

- **Foreign subject: wrong answer, NEEDS-CHANGE, introduced.** Location: `crates/spindle-host/src/host.rs:507` (`Routed::source`) together with `crates/spindle-kernel/src/supported.rs:250` (`foreign_affects`).
  - **What happens:** when a command's `creates:` or `updates:` targets an entity another component owns, the host reads that entity from the owner's store but writes it into the running component's store. After that, every read of the entity (including the running component's own guards) goes to the owner's store, where the row is not.
  - **At the base:** `ChargeDirect` was refused by name (`foreign_guard`). The wrong "charged" answer is new in this unit.
  - **What reaches it:** any ESS model in which a component accepts a command whose subject belongs to a domain another component owns. ESS 0.99.1 compiles that, and `admit` lists the command as served. I did not find a consumer model that does this.
  - **Fix:** refuse a subject over another component's entity by name in every run, the way an `affects:` entry over one is refused now (D2 A).
- **Check-then-commit race: INFEASIBLE, introduced.** Location: `crates/spindle-host/src/host.rs:524` (`Routed::unmoved`) and `:668` (`commit`).
  - **What happens:** another writer that changes the owner's store between the version check and the commit leaves a stale decision kept.
  - **What reaches it:** nothing inside one process. Across processes, a PostgreSQL-bound served `run` takes no single-process lock (`crates/spindle-cli/src/serve.rs:287-294`), so the claim in the design row and the doc comment, "as a state directory has one Spindle process", does not cover PostgreSQL. I found no supported setup with two writers on one database.

**5. Attacked and not broken**
- Parity with the interpreter for count, forall, exists true/false, the owner with no rows, and rows released or re-levelled between commands: memory and SQLite.
- A split run refusing by name, on SQLite.
- The two-step owner check when nothing writes in the window (the unit's own test covers this).
- Callers in `spindle-control` and `spindle-embed` are unchanged (no diff); `component.rs` is untouched.
- `play --component` uses `Host::component` plus `Surface::new`, which reach the running component alone.
- `Routed::counts` is reached only when a foreign subject's identity is minted, which is the first finding above.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`

```findings
- file: crates/spindle-host/src/host.rs
  line: 507
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a command whose creates/updates subject is another component's entity writes it to the running store while every read of it is routed to the owner, so the running component's own row-set guard answers charged where ESS's interpreter answers on-hold
- file: crates/spindle-host/src/host.rs
  line: 524
  category: concurrency
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: a write of the owner's store between Routed's version check and the running store's commit leaves a stale charge kept; only another process reaches it, and a PostgreSQL-bound served run takes no single-process lock, contrary to the doc's "a state directory has one Spindle process"
```
