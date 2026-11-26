---
format: aep.planning-md/3
id: review-result:wave27-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U4
relations:
- reviews: story:map-input-guard-facts
revision: 1
---
```
unit: story:map-input-guard-facts, head 2e23a0824 (findings cover it; my tests committed on top as ec6698b10)
verdict: CONFIRMED
cases: executed 409→416, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths, all in units/U4/adv-scratch/
needs-coordinator: none
```

**Summary:** the implementor's open gap is real. A quantifier over a `Map<String, Timestamp>` input now reaches a decision, and the decision is wrong. The kernel compares each timestamp by its bytes and opens the sheet (writes the row). ESS's interpreter compares by instant and refuses with `early`. Every other `Map` position I tried matches the interpreter.

**1. Diff stat** (`git --no-pager diff --stat 2e23a0824 HEAD`; all test paths):
```
 .../fixtures/adversary_w27_u4/domains/ledger.yaml  | 219 +++++++++++++++++
 .../tests/fixtures/adversary_w27_u4/system.yaml    |  26 ++
 .../tests/spindle-kernel/adversary_w27_u4.rs       | 270 +++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   1 +
```

**2. Cases added** in `<local path>`, fixture `crates/spindle-kernel/tests/fixtures/adversary_w27_u4/`. Each case checks the kernel against `ess_conformance::interpret` and also against a literal list of expected outcomes.

| Case | What it checks | State now |
|---|---|---|
| `a_timestamp_map_value_is_ordered_by_its_instant` | `Map<String, Timestamp>`, `exists … s < "2020-01-01T00:00:00Z"` | **red** |
| `a_timestamp_list_value_is_ordered_by_its_instant` | the same guard over `List<Timestamp>` (control) | green |
| `a_map_inside_a_struct_answers_as_the_interpreter` | `card.scores.count`, and `exists` over `card.scores` | green |
| `a_list_of_maps_answers_as_the_interpreter` | `List<Map>` with `r.count > 1` | green |
| `a_map_of_maps_answers_as_the_interpreter` | `Map<String, Map>` with `g.count`, and a nested `exists` | green |
| `a_newtype_over_an_optional_map_answers_as_the_interpreter` | newtype over a map: `{}`, two entries, key left out, `null` | green |
| `a_stored_map_and_an_input_map_under_input_answer_as_the_interpreter` | stored `tallies.count` (present and absent) in `when_subject:`, and `exists` over `input.extra` | green |

The first run failed for a fixture reason, not a finding: the compiler refuses a guard that reads a `Map` caller attribute with anything but `==`/`!=`, and refuses `>` on a `Duration`. I removed both from the fixture. The red output from the first run that compiled, with the module alone:
```
---- adversary_w27_u4::a_timestamp_map_value_is_ordered_by_its_instant stdout ----
assertion `left == right` failed: request 0: probe.ledger.StampMap {"id":"7d1e0000-0000-4000-8000-000000000001","stamps":{"a":"2020-01-01T00:30:00+01:00"}}: the kernel selects `opened`, the interpreter `early`
  left: "opened"
 right: "early"
test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 451 filtered out; finished in 0.00s
```

**3. Suite runs** (after the cases existed):
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 101, lib `12 passed`, integration `FAILED. 403 passed; 1 failed; 54 ignored`, 13 s wall.
- The same with `-- --skip adversary_w27_u4` (my 7 cases left out): exit 0, `12 passed` and `397 passed; 0 failed; 54 ignored; 7 filtered out`, 13 s wall.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-kernel -- --check`: exit 0.
- No new test takes over 5 s.

**4. Finding**
- **What was measured:** `adversary_w27_u4.rs:167`, the assertion inside `answered_as_the_interpreter` (`:130`).
- **Cause:** `crates/spindle-kernel/src/facts.rs:221-236`. `Facts::declared` resolves `stamps.0` with `resolve_path` only. That fails at a map's position, so the value is treated as plain text and compared by bytes. ESS's `declared_primitive` (`ess-conformance/src/input.rs:193`) falls back through `map_value` to the map's value type.
- **The fix:** the same fallback in `declared`: find a segment that is a number, check that the path before it resolves to a `Map` (through `Optional` or a newtype), and resolve the rest against the map's value type.
- **Who reaches it:** any consumer that declares a `Map<String, Timestamp>`, or a map whose values are a newtype over `Timestamp`, and compares its values with `<`/`>` in a guard. The compiler accepts this, and the served route reaches it.
- **Origin: introduced.** At the base the kernel refused this command as undecidable and wrote nothing. This diff makes it decide, wrongly, and commit a row. I worked this out from `git show <base>:facts.rs`; I did not run the base.
- Covers commit 2e23a0824.

**5. Attacked and not broken**
- A map inside a struct, a list of maps, a map of maps, and a nested quantifier.
- A newtype over an optional map: empty, absent and `null`.
- A stored map read by `when_subject:`, and an input map read there under `input.`.
- Key order: the kernel sorts by `String`, the same order as ESS's `BTreeMap`.
- A `Map<String, Duration>` and a `Map` caller attribute: the compiler refuses both, so no guard reaches them.
- The implementor's tests check literal outcomes against the interpreter; none of them would pass with every function returning a default.

**6. Paths written outside the worktree**, all under `<local path>`: `run1.log`, `run2.log`, `suite.log`, `suite-before.log`, `head.rs`, `tail.rs`, `trim.rs`, `commit-msg.txt` (8 files in one directory).

```findings
- file: crates/spindle-kernel/src/facts.rs
  line: 221
  category: acceptance
  severity: blocker
  verdict: CONFIRMED
  origin: introduced
  message: Facts::declared does not resolve a map's value at an ordinal as ESS's declared_primitive does, so a quantifier over a Map<String, Timestamp> compares by bytes and commits `opened` where the interpreter refuses `early`
```
