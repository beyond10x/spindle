---
format: aep.planning-md/3
id: review-result:wave30-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U1
relations:
- reviews: story:view-filter-nested-element-quantifier
- reviews: story:view-filter-map-value-order
revision: 1
---
unit: wave 30 U1, story:view-filter-nested-element-quantifier and story:view-filter-map-value-order, findings cover the head 82d4364a (tests committed on top as bae972a3)
verdict: CONFIRMED (2 test gaps of severity note; no wrong answer found)
cases: executed 199→208, red 0
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 scratch directory, plus one managed probe worktree (now marked finished); both are listed in part 6
needs-coordinator: none

The unit holds up: no case made Spindle answer differently from ESS's interpreter. What I found are two places where the unit's own tests would stay green if the code broke, and docs that list fewer accepted filter forms than the code accepts.

**1. `git --no-pager diff --stat 82d4364a bae972a3`**
```
 .../fixtures/adversary-w30-u1/domains/probe.yaml   | 311 +++
 .../tests/fixtures/adversary-w30-u1/system.yaml    |  19 +
 .../tests/spindle-host/adversary_w30_u1.rs         | 417 +++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
```
All four are test paths. No implementation file was touched.

**2. Cases added** in `<local path>`. Fixture: `tests/fixtures/adversary-w30-u1`. Each case is compared with the interpreter on the memory and SQLite stores, on the host and through its reader. All 9 are green now. Expected rows are written as literals worked out by hand.
- `nested_element_quantifiers_three_deep_and_mixed_are_served_as_the_interpreter`: quantifiers three levels deep, `forall` inside `exists` and the reverse, empty inner and outer lists, `w == g.name`.
- `a_shadowed_binder_reads_the_innermost_element`: `exists g in g.slots` inside `exists g in groups`.
- `optional_lists_and_maps_never_answer_differently`: an absent `Optional` list (as a field and as an element member) and an absent `Optional` map. ESS and Spindle both refuse the read.
- `an_optional_map_held_is_served_as_the_interpreter`: an `Optional<Map<String, Timestamp>>` guarded by `not defined`.
- `map_value_instants_spelled_differently_are_served_as_the_interpreter`: `==`, `!=`, `>`, `>=`, `not <`, a literal with fractional seconds, a literal with an offset, map value against a `Timestamp` field, equal instants written as `+01:00`, `-05:00`, `.000Z` and `-00:00`.
- `map_values_of_other_types_are_served_as_the_interpreter`: map values of type `Decimal`, `Integer`, `String` holding integers as text, and `Duration`.
- `maps_inside_elements_and_of_structs_are_served_as_the_interpreter`: a map inside a list element, and a map of structs (a member read, and a list member walked).
- `an_aggregate_where_over_a_map_and_a_nested_list_counts_as_the_interpreter`: an aggregate `count` whose `where:` uses both new forms.
- `a_quantifier_over_a_map_parameter_is_refused_whole`: decision D2.

No case was red when written. The first runs failed only on my own mistakes (an unquoted YAML type, and a `3` vs `3.0` number comparison), both fixed before the run that counts.

**3. Suite runs** (all after the cases existed):
- `cargo test --locked -p spindle-host --no-fail-fast` → exit 0, 19 s wall. Lib: 1 passed. Integration binary: `207 passed; 0 failed; 14 ignored`. Before the last case was added it was 206 passed, 0 failed.
- The same command with `-- --skip adversary_w30_u1` → `198 passed; 14 ignored; 9 filtered out`. This is the source of the `<before>` count (199 = 198 + 1 lib test).
- `cargo clippy --locked -p spindle-host --all-targets -- -D warnings` → exit 0.
- `cargo fmt -p spindle-host --check` → exit 0.
- No new test takes over 5 s; the whole binary runs in 10.3 s.

Mutant probes, run in the managed worktree `spindle-w30-u1-adv-mutant` at 82d4364a with my tests copied in:
- **M1**: drop `typed.row &&` from the map check, `view.rs:476`. Result: `205 passed; 1 failed`. Only `a_quantifier_over_a_map_parameter_is_refused_whole` failed; the unit's own suite stayed green.
- **M2**: drop `.rev()` in `Scope::walked`, `view.rs:369`. Result: `205 passed; 1 failed`. Only `a_shadowed_binder_reads_the_innermost_element` failed; the unit's own suite stayed green.
- **M4**: make the `Optional` arm of `map_value` return `None`. Result: 207 passed, so every test stays green. This looks equivalent for the forms tested, because the resolved type of an `Optional` field already arrives without the `Optional` wrapper.

**4. Findings that are judgement or test gaps**
- `crates/spindle-host/src/view.rs:476`: decision D2 (a map parameter stays refused) had no test anywhere in the unit's suite; mutant M1 survived it. My case now pins it. What reaches it: a view with a `Map` parameter under `exists`/`forall`. The interpreter answers such a view (`MapParam` with `ps` given returned all 5 rows), so a regression would change behaviour without any test noticing. Introduced, CONFIRMED.
- `crates/spindle-host/src/view.rs:369`: choosing the innermost of two binders with the same name was untested; mutant M2 survived, and in that case the view would be refused although ESS answers it. My case pins it. What reaches it: a filter that reuses a binder name, which ESS accepts. Introduced, CONFIRMED.
- `website/docs/concepts/specification.md:173` and the `unsupported` doc comment and refusal text in `view.rs:65-103` list "a map field of the row" and "a list member of the element". The code also serves a map member of a list element (`exists v in g.stamps`) and a map of structs (`i.at`, `exists w in i.words`). It answers those correctly, matching the interpreter, but the docs list fewer forms than are accepted. Introduced, CONFIRMED, note.

**5. Attacked and could not break**
- Three-level nesting, mixed `exists`/`forall`, empty lists, a shadowed binder: all match the interpreter.
- Equal instants written differently, mixed offsets, fractional seconds down to sub-millisecond (`.5001` > `.5`), `-00:00`: all match.
- `Decimal`, `Integer`, numbers stored as `String`, and `Duration` map values: all match.
- An `Optional` map, absent or present; absent `Optional` lists: Spindle refuses or answers exactly where the interpreter does.
- The same filters inside an aggregate's `where:`: counts match.
- Contract drift: the only consumer of the changed refusal text is `ess/fixtures/observe/served/notes.json`, which the unit already updated.

**6. Paths written outside the worktree**
- `<local path>`: run1–5.log, probe1.log, suite-after.log, suite-final.log, suite-before.log, mutant-m1.log, mutant-m2.log, mutant-m4.log, clippy.log, head.rs, commit-msg.txt.
- `<local path>`: the managed probe worktree. It is clean and marked finished with `worktree finish --discard-cache`; it is left for `worktree gc` to remove.

**7. Findings block**
```findings
- file: crates/spindle-host/src/view.rs
  line: 476
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: dropping the typed.row guard that keeps a Map parameter refused (decision D2) leaves the unit's suite green; only adversary_w30_u1::a_quantifier_over_a_map_parameter_is_refused_whole catches it
- file: crates/spindle-host/src/view.rs
  line: 369
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: dropping .rev() in Scope::walked, so a shadowed binder resolves to the outer element, leaves the unit's suite green; only adversary_w30_u1::a_shadowed_binder_reads_the_innermost_element catches it
- file: website/docs/concepts/specification.md
  line: 173
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the docs and the refusal text name only a map field of the row and a list member of an element, while a map member of a list element and a map of structs are also served
```
