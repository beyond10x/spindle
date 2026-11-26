---
format: aep.planning-md/3
id: review-result:wave27-adversary-U5-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U5
relations:
- reviews: story:integer-identity-any-spelling
revision: 1
---
```
unit: story:integer-identity-any-spelling (U5), findings cover head 01c015489 (unit head c66d1db0d plus my test commit)
verdict: CONFIRMED (one pre-existing parity gap; no defect in the unit's diff)
cases: executed 410→415, red 1
origin: introduced 0 / pre-existing 1 / undecided 0
wrote-outside-worktree: 6 paths (part 6)
needs-coordinator: none
```

I couldn't break the unit. Every Integer path I tried behaves as ESS 0.99.1's interpreter does, and four new tests now cover them. The one red case is a Decimal identity, which the story doesn't cover and which fails the same way at the base commit.

**1. Diff stat** (`git --no-pager diff --stat c66d1db0d HEAD`). Both paths are test files.
```
 .../tests/spindle-kernel/adversary_w27_u5.rs       | 613 +++++++++++++++++++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   1 +
```

**2. Cases added** in `<local path>`, committed as 01c015489. Every case is compared step by step with `ess_conformance::interpret`. All five failed on a copy of the base commit, so the four green ones would catch a revert of the unit.

| Test | What it asserts | Now |
|---|---|---|
| `a_newtype_integer_identity_names_one_row_whatever_its_spelling` | an identity typed as a newtype over Integer: `1.0`/`1`, `1e0`, `10E-1`, `0.1e1`, `-0`/`0`/`-0.0`, `-7.0`/`-7` each give one row keyed as the integer; a second create is refused; the event payload holds the integer | green |
| `a_nested_integer_is_the_integer_and_a_decimal_keeps_its_spelling` | Integers inside a struct member, a map value and a union payload (under both `value` and `content`) are held and published as integers; a Decimal, a Binary64 and a `List<Decimal>` beside them keep `2.0` and `[7.0]` | green |
| `a_related_read_finds_its_row_whatever_the_integer_spelling` | `when_related` with `exists: false` and `{related: {via: input.bin_no}}` find the row by either spelling; the reference is stored as the integer | green |
| `a_durable_store_finds_an_integer_identity_whatever_its_spelling` | a SQLite store reopened between create and update finds the row by either spelling | green |
| `a_decimal_identity_names_one_row_whatever_its_spelling` | a Decimal identity created as `1.0` and updated as `1` names one instance, as in ESS | **red** |

Red output from the first run of the module on its own, before the suite:
```
step 1 probe.bin.Reweigh {"grams":1,"note":"again"}: the kernel answers Err(Undeclared { command: "probe.bin.Reweigh", reason: "no `probe.bin.Scale` has the identity 1, and the command declares neither `unknown_instance:` nor `wrong_state:`" }), the interpreter Ok(("reweighed", None))
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 452 filtered out; finished in 0.03s
```
On the base copy (`git archive c61a5965` in scratch, plus this module) all five fail, and the Decimal case gives the same message. The four green cases fail there with errors such as `no 'probe.bin.Bin' has the identity 1` and `left: ("no-bin", ...) right: ("filled", None)`. The whole module runs in about 0.03 s, so no test is near 5 s.

**3. Suite run**, after the cases existed.
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 101 after 18 s.
  - Unit tests: `ok. 12 passed`.
  - Integration binary: `FAILED. 402 passed; 1 failed; 54 ignored ... finished in 6.65s`. The one failure is the Decimal case.
- `executed <before>` is 410: the 415 executed minus my 5 cases. I had no implementor `cases:` line to take it from.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings`: exit 0, after I fixed one lint in my own file.
- `cargo fmt -p spindle-kernel -- --check`: exit 0.

**4. Judgement findings**
- **Decimal identity.** `crates/spindle-kernel/src/typed.rs:53` rewrites only values typed Integer, and the store keys a row by the identity's JSON text. A Decimal identity sent as `1.0` and `1` is therefore two rows in Spindle and one in ESS.
  - What reaches it: any model that declares a Decimal identity, which ESS allows.
  - Verdict CONFIRMED, origin pre-existing (reproduced on the base copy).
  - The fix is out of this story's scope, which only covers Integer. Since a Decimal value must not be rewritten, the fix would key rows by the value ESS reads rather than by the JSON text.
- **Misleading test name (note, no failing case).** The implementor's test `an_integer_identity_past_64_bits_is_held_as_written` now asserts that such values are refused, following the amended acceptance. Its name still says "held as written", and so does the story text at revision 9 in this worktree.

**5. Attacked and not broken**
- Every row write and read path goes through `execute_refusing`; none bypasses the rewrite.
- The address, `before_row`, related reads (single and list) and the creation check all read the rewritten input.
- Caller attributes are rewritten; the condition values passed to guards still compare by value.
- Rules in host decision files: an integer rule still matches the rewritten input.
- View parameters are compared by value. `Host::view` is only called with identities the code builds itself.
- No `arbitrary_precision` in the kernel's build, so ESS and the kernel read numbers past 2^53 the same lossy way.
- Values past i64 are refused on both sides, before the rewrite runs.
- The rewrite's depth limit can't fire on anything the type check accepted first.
- Map keys typed Integer only accept the canonical spelling.
- Mutants my tests would catch and the unit's suite wouldn't:
  - dropping the struct, map or union branch of the rewrite;
  - swapping the union's `value`/`content` payload key;
  - extending the rewrite to Decimal.

**6. Paths written outside the worktree**, all under `<local path>`:
- `base/`: the base-commit copy, 873 MB including the `target/` directory the pool built there locally. This one should be cleaned up.
- `base.log`, `own.log`, `suite.log`, `clippy.log`, `commit-msg.txt`

The session lease `adv-w27-u5` is released.

**7. Findings**
```findings
- file: crates/spindle-kernel/src/typed.rs
  line: 53
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: a Decimal identity spelled 1.0 and 1 is one instance in ESS 0.99.1's interpreter and two in the kernel, which keys rows by the identity's JSON text and rewrites only Integer (adversary_w27_u5::a_decimal_identity_names_one_row_whatever_its_spelling)
```
