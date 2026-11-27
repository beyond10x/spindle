---
format: aep.planning-md/3
id: review-result:wave30-adversary-U6-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U6
relations:
- reviews: story:decimal-identity-any-spelling
revision: 1
---
unit: story:decimal-identity-any-spelling (wave 30 U6), head 01f41648; my cases committed on top as a3d1aa71 on unit/w30-decimal
verdict: CONFIRMED
cases: executed 638→645, red 3
origin: introduced 4 / pre-existing 0 / undecided 0
wrote-outside-worktree: 10 paths (part 6)
needs-coordinator: whether host reads by identity (view, view through a reference, history) are fixed in this unit or get their own story. The brief listed "view reads by identity" as not changed, and these three reads worked at the base.

**1. `git --no-pager diff --stat 01f41648..a3d1aa71`**
```
 .../tests/spindle-host/adversary_w30_u6.rs         | 276 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 .../tests/spindle-kernel/adversary_w30_u6.rs       | 270 ++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   1 +
 4 files changed, 549 insertions(+)
```
All four are test files. The working tree is clean after the commit.

**2. Cases added**

`crates/spindle-host/tests/spindle-host/adversary_w30_u6.rs`. The model: scales keyed by a `Decimal` weight, and pans that reference a scale by its weight. Each case first creates the scale by command with weight `1.0`.
- `a_view_reads_through_a_reference_spelled_as_the_scale_was_set`: **red**. A pan with weight `1.0` lists `scale_note: "top"`.
- `a_view_reads_a_scale_by_the_spelling_it_was_set_with`: **red**. `Host::view(Scales, 1.0)` finds the scale.
- `a_history_reads_a_scale_by_the_spelling_it_was_set_with`: **red**. `Host::history(Scale, 1.0)` answers `[ScaleSet]`.
- `a_view_reads_a_scale_set_at_1_0_by_1`: green, kept. It was red at the base, which this unit fixed.

First run, these four cases alone (`cargo test --locked -p spindle-host --test spindle-host adversary_w30_u6`):
```
test adversary_w30_u6::a_view_reads_a_scale_by_the_spelling_it_was_set_with ... FAILED
test adversary_w30_u6::a_view_reads_through_a_reference_spelled_as_the_scale_was_set ... FAILED
test adversary_w30_u6::a_history_reads_a_scale_by_the_spelling_it_was_set_with ... FAILED
test adversary_w30_u6::a_view_reads_a_scale_set_at_1_0_by_1 ... ok
...adversary_w30_u6.rs:233:5: the scale set at 1.0 is read by 1.0
...adversary_w30_u6.rs:217:5: assertion `left == right` failed: the pan names the scale set at 1.0 by 1.0
  left: [{"pan_id": String("p1")}]
 right: [{"pan_id": String("p1"), "scale_note": String("top")}]
...adversary_w30_u6.rs:271:5: assertion `left == right` failed: the scale set at 1.0 has its history at 1.0
  left: []
 right: ["probe.pans.ScaleSet"]
test result: FAILED. 1 passed; 3 failed; ... finished in 0.01s
```
The same module run in a managed worktree at base 8eaea47a: the three red cases pass and `a_view_reads_a_scale_set_at_1_0_by_1` fails. So all three red cases are introduced by this unit.

`crates/spindle-kernel/tests/spindle-kernel/adversary_w30_u6.rs`. Every case is checked step by step against `ess_conformance::interpret`, and all three are green:
- `every_spelling_of_one_value_names_one_scale`: these pairs each name one scale: `10E-1`/`1`, `1.000`/`1e0`, `0.1e1`/`1.0`, `-0.0`/`0`, `-0`/`0.0`, `0.10`/`1e-1`, `2.5`/`25E-1`, `-1.0`/`-1`, `1e19`/`10000000000000000000`, `-9223372036854775808.0`/`-9223372036854775808`, and `1`/`1.00000000000000001` (both sides read it as the same binary64).
- `two_values_name_two_scales`: these pairs each name two scales: `1`/`1.0000000000000002`, `0.1`/`0.10000000000000002`, `9007199254740993`/`9007199254740993.0`, `u64::MAX`/`2^64`, `i64::MAX`/`2^63`.
- `a_related_read_finds_a_decimal_scale_whatever_its_spelling`: a `when_related:` read finds the scale when the input is spelled differently from the stored key.

My first version of `two_values_name_two_scales` went red for an unrelated reason, so I rewrote it without that step. It used `Note` on an unknown scale: the interpreter answers no outcome and the kernel refuses with `Undeclared`, whatever the spelling. That gap is not this unit's and I did not chase it.

Mutant probe, run in a separate managed worktree at the head:
- **Mutant 1**: drop the `Spelled` wrapper in `reaching`. Seven cases fail, the unit's own included, so the unit's suite catches it.
- **Mutant 2**: `Spelled::load` forwards the identity unspelled (scratch `mutant2.diff`). Every test of the unit passes (13 passed). Only my related-read case fails:
```
step 1 probe.weigh.AddPan {"grams":1.0,"pan_id":"p1"}
  left: ("no-scale", Some("probe.weigh.NoScale"))
 right: ("added", None)
test result: FAILED. 13 passed; 1 failed
```

**3. Suite runs, after my cases existed**
- `cargo test --locked -p spindle-host --no-fail-fast`: exit 101. Unit tests `1 passed`; `tests/spindle-host`: `FAILED. 199 passed; 3 failed; 14 ignored` (10.26 s). The 3 failures are my red cases.
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 0. `15 passed`; `ok. 427 passed; 0 failed; 54 ignored` (13.07 s).
- The before count comes from the same suites with my modules deselected (`-- --skip adversary_w30_u6`): host `1 + 198` (4 filtered out), kernel `15 + 424` (3 filtered out). Total 638 before, 645 after.
- `cargo clippy --locked -p spindle-kernel -p spindle-host --all-targets -- -D warnings`: Finished, no warnings. `cargo fmt -p spindle-kernel -p spindle-host --check`: exit 0.
- Every new case finishes in under 0.1 s.

**4. Findings**

| file:line | finding | verdict | origin | what reaches it |
|---|---|---|---|---|
| crates/spindle-host/src/host.rs:2821 | A view field read through a reference loads the row by the field's own spelling (`1.0`), but the row is now keyed `1`. The field answers absent, while ESS and the base answered `"top"`. | NEEDS-CHANGE | introduced | `listed_with` / ingress view lists. Same path in `Reader::list_with` (`host/reader.rs:348`). A world row hits it too: its reference field keeps `1.0` while its identity is now keyed `1`. |
| crates/spindle-host/src/host/range.rs:540 | History compares `held == identity` as JSON. The write records `1`, the caller asks for `1.0`, and the history comes back empty. | NEEDS-CHANGE | introduced | `GET /api/history?identity=1.0` (`spindle-ingress/src/console.rs`), through both `Host::history` and `Reader::history`. |
| crates/spindle-host/src/host.rs:1955 | `Host::view` loads by the raw identity, so a scale set at `1.0` is not found by `1.0`. | NEEDS-CHANGE | introduced | Public API. The only caller in the repository is `spindle-cli` (string ids), so this one is reached less. |
| crates/spindle-kernel/src/store.rs:721 | Nothing in the unit's suite checks that `Spelled::load` spells the identity: mutant 2 passes all of it. My kept related-read case now catches it. | CONFIRMED | introduced | Any related read or `existing()` check whose input is spelled differently from the stored key. |
| crates/spindle-kernel/src/execute.rs:1839 | The unknown-instance refusal now names the spelled identity (`identity 1`) where the caller sent `1.0`. | CONFIRMED | introduced | Any update of an unknown `Decimal` instance with no `unknown_instance:` outcome. Note-level only; no test written. |

Fix for the first three: put the identity through `spindle_kernel::one_spelling` before each host read by identity. That means `Host::view`, `references`/`Far::referenced` (the `far` value), and the history comparison: spell the asked identity in `range::history`.

**5. Attacked and not broken**
- Exponent forms, trailing zeroes, `-0`/`-0.0`, the ends of i64 and u64, 2^53+1, and precision loss through f64: all agree with the interpreter.
- An `affects:` entry over the subject's own entity still leaves the subject out.
- The guards' facts compare by value. `commit_all`'s rewrite path holds.
- A `when_related` read by any spelling finds its row.
- NaN and infinity cannot be reached: serde_json does not parse them.
- An `Integer` identity is unaffected (the w27_u5 cases are green on both mutants).

**6. Paths written outside the worktree**
- `<local path>` and its files: `commit-msg.txt`, `host-new.log`, `kernel-new.log`, `host-suite.log`, `host-skip.log`, `kernel-suite.log`, `kernel-skip.log`, `mutant.diff`, `mutant2.diff`.
- `<local path>` and `<local path>`: managed worktrees I created for the base comparison and the mutants. I reverted my probe edits in both and marked them finished with `worktree finish --discard-cache`; removing them is left to `worktree gc`.

**7.**
```findings
- file: crates/spindle-host/src/host.rs
  line: 2821
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a view field read through a reference loads the referenced row by the field's spelling 1.0 while the row is now keyed 1, so the field answers absent where the base and ESS answered the row
- file: crates/spindle-host/src/host/range.rs
  line: 540
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: an instance's history compares the asked identity with the written one as JSON, so GET /api/history?identity=1.0 answers empty for a row a command created as 1.0
- file: crates/spindle-host/src/host.rs
  line: 1955
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: Host::view loads by the raw identity, so a row created by a command as 1.0 is no longer found by 1.0
- file: crates/spindle-kernel/src/store.rs
  line: 721
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: Spelled::load forwarding the identity unspelled passes every test of the unit; only the kept adversary case a_related_read_finds_a_decimal_scale_whatever_its_spelling catches it
- file: crates/spindle-kernel/src/execute.rs
  line: 1839
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the unknown-instance refusal names the spelled identity 1 where the caller sent 1.0
```
