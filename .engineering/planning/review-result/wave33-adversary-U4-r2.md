---
format: aep.planning-md/3
id: review-result:wave33-adversary-U4-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 33 U4
relations:
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
unit: story:entity-invariant-over-stored-rows (U4 round 2); covers head bbe4b908 (fix 0ece1f8a plus my test commit)
verdict: CONFIRMED
cases: executed kernel 524→529, cli 739→741, red 3 (cli has 3 more reds that were already red on the base: no `ui/dist`)
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 13 paths (part 6)
needs-coordinator: none

The fix (`OneLine`) only escapes `\n` and `\r`. Other line breaks and terminal control characters in an invariant's statement still reach the misfit line raw. The escaping also can't be reversed: a real line break and a written `\n` print the same line. ESS 0.99.2 compiles every one of these statements, so the reds are real (proved in the cases). Everything else I tried held, through `run` and on PostgreSQL as well.

**1. Diff (my commit bbe4b908, test files and fixtures only)**
```
 crates/spindle-cli/tests/fixtures/adversary-w33-u4-after/domains/shelf.yaml   |  32 ++
 crates/spindle-cli/tests/fixtures/adversary-w33-u4-after/system.yaml          |  19 ++
 crates/spindle-cli/tests/fixtures/adversary-w33-u4-before/domains/shelf.yaml  |  47 +++
 crates/spindle-cli/tests/fixtures/adversary-w33-u4-before/system.yaml         |  21 ++
 crates/spindle-cli/tests/spindle-cli/adversary_w33_u4.rs                      | 127 +++++++
 crates/spindle-cli/tests/spindle-cli/main.rs                                  |   1 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w33_u4.rs                | 365 ++++++++++
```
No implementation path was touched, and the round-1 cases are kept as they were.

**2. Cases added**

In `<local path>`:
- `adv2_every_unicode_line_break_in_an_invariant_keeps_the_misfit_on_one_line` (:300): **red**. A statement holding VT, FF, NEL, U+2028 or U+2029 must not have that character raw in the misfit line.
- `adv2_no_control_character_of_an_invariant_is_written_raw` (:340): **red**. ESC, BEL, BS, NUL and the C1 CSI must not be written raw (tab is allowed).
- `adv2_a_line_break_and_a_written_backslash_n_misfit_apart` (:379): **red**. Two different statements must print two different lines.
- `adv2_an_invariant_over_an_optional_field_a_row_lacks_fits` (:399): green, kept. A row without `note` and a row with `note: null` both fit `note.count <= 4` and `note != 'abc'`; this matches `at_rest`.
- `adv2_invariant_field_and_state_misfits_of_one_entity_each_take_a_line` (:562): green, kept. Three invariant misfits, a removed field, a required field and a removed state are reported together, one line each. An invariant over the newly required `isbn` adds no line (its value is unknown).
- `adv2_invariant_field_and_state_misfits_match_sqlite_on_postgres` (:578, `#[ignore]`, selected by the lane through `~on_postgres`): green, kept, 0.078 s. PostgreSQL gives the same lines as SQLite, and `fit` leaves the record unchanged.

In `<local path>`, with the new fixtures `adversary-w33-u4-before` and `adversary-w33-u4-after`:
- `adv2_a_refused_stdin_run_prints_one_line_per_misfit`: green, kept, 0.024 s. The run exits 3 and stdout is exactly the refusal line plus 4 misfit lines (3 invariants, one holding `\n`, and state Lent).
- `adv2_a_row_stored_without_its_optional_field_fits_an_invariant_on_it`: green, kept, 0.026 s. A book a command stored without its note starts under `note.count <= 4`.

The first two runs of these cases alone were red for the wrong reason: my fixtures did not compile (`unreachable_state`, `missing_causation`, `empty_change`, and an infix `or` that ESS does not parse). I fixed the fixtures and reran. Red output of the run with the cases alone (`cargo nextest run --locked -p spindle-kernel --test spindle-kernel -E 'test(~adversary_w33_u4::adv2_)'`, exit 100):
```
a line break other than \n and \r is written raw into the misfit's line: [
    "\\v: \"keep.shelf.Book invariant `title != 'a\\u{b}b'`: the invariant does not hold; stored rows breaking it: 1\"",
    "\\f: ... 'a\\u{c}b' ...", "\\N: ... 'a\\u{85}b' ...", "\\L: ... 'a\\u{2028}b' ...", "\\P: ... 'a\\u{2029}b' ...",
]
a control character of the statement is written raw: [
    "\\e: \"keep.shelf.Book invariant `title != 'a\\u{1b}b'`: ...\"", "\\a: ...'a\\u{7}b'...", "\\b: ...'a\\u{8}b'...", "\\0: ...'a\\0b'...", "\\x9b: ...'a\\u{9b}b'...",
]
assertion `left != right` failed: two statements, one misfit line: "title != 'a\nb'" and "title != 'a\\nb'"
  left: "keep.shelf.Book invariant `title != 'a\\nb'`: the invariant does not hold; stored rows breaking it: 1"
 right: "keep.shelf.Book invariant `title != 'a\\nb'`: the invariant does not hold; stored rows breaking it: 1"
Summary [0.049s] 5 tests run: 2 passed, 3 failed, 595 skipped
```
CLI cases alone: `Summary [0.027s] 2 tests run: 2 passed, 832 skipped`, exit 0.
PostgreSQL case alone, run as one slot job inside the lane cluster (`slot --timeout 30m --tokens 4 sh scripts/postgres-lane.sh lane -- cargo nextest run --locked --profile postgres ... --run-ignored all -E 'test(=adversary_w33_u4::adv2_invariant_field_and_state_misfits_match_sqlite_on_postgres)'`): `1 test run: 1 passed`, the cluster was removed, exit 0.

**3. Suite runs (after the cases existed)**

| Command | Exit | Summary line |
|---|---|---|
| `cargo test --locked -p spindle-kernel --no-fail-fast` | 101 | `test result: FAILED. 526 passed; 3 failed; 71 ignored ... finished in 7.47s` (the 3 are mine); unit tests `15 passed` |
| `cargo test --locked -p spindle-cli --no-fail-fast` | 101 | `test result: FAILED. 738 passed; 3 failed; 93 ignored ... finished in 30.25s` (the 3 are the `deploy::` and `observe_panel::` cases already red on the base) |
| `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings` | 0 | |
| `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` | 0 | |
| `cargo fmt -p spindle-kernel --check` | 0 | |
| `cargo fmt -p spindle-cli --check` | 0 | |

The "before" counts are the implementor's last runs: kernel 524 passed (`r1-test-kernel.log`), cli 736 passed plus 3 failed (`green2-test-spindle-cli.log`). No new test runs over 5 s.

**4. Findings** (all on head bbe4b908)

| # | Finding | Verdict, origin, severity | What reaches it |
|---|---|---|---|
| F1 | `crates/spindle-kernel/src/fit.rs:394`: `OneLine` escapes only `\n` and `\r`, so VT, FF, NEL, U+2028 and U+2029 in a statement reach the line raw. This goes against its own doc ("keeps its line one line") and against D2 A. | CONFIRMED, introduced, warning | A specification author writes the character in a YAML double-quoted invariant; ESS 0.99.2 compiles it. No fixture in the repository does this. |
| F2 | Same line: ESC, BEL, BS, NUL and C1 controls reach the stdout of `run` raw. ESC can move the cursor or clear the line. | CONFIRMED, introduced, note | Same path as F1. |
| F3 | Same line: the escaping is not injective. A statement with a line break and one with a written `\n` print identical lines. | CONFIRMED, introduced, note | Same path as F1. |

Fix I'd name, not applied: escape the backslash itself, and every character where `char::is_control()` or that is U+2028/U+2029, for example as `\u{..}` (ESS's `TextOperand` Display already writes literals with `{:?}`).

**5. Attacked and held**
- The misfit count equals what `at_rest` refuses for a missing optional field or a `null` one (SQLite, and a command-written row through `run`).
- Invariant misfits sit beside Removed, Required and State misfits; a Required field's invariant is left unknown and adds no line.
- PostgreSQL gives the same misfit lines as SQLite, and the record is unchanged after a refusal.
- `run` stdin path: exit 3, exactly one stdout line per misfit, and a `\n` statement stays on its line.
- `Misfitted` and `HostError::Misfit` both print through `Misfit`'s Display, so they have no separate escape path.

**6. Paths written outside the worktree**
- In `<local path>`: `kernel-alone.log`, `kernel-alone-2.log` to `kernel-alone-5.log`, `cli-alone.log`, `cli-alone-2.log`, `pg-alone.log`, `suite-kernel.log`, `suite-cli.log`, `clippy-kernel.log`, `clippy-cli.log`, `commit-msg.txt`.
- `kernel-alone.log` was written first into `.../U4/adv-scratch/` (the brief's name for the scratch directory) and then moved to `adv2-scratch`.
- The lane cluster lived under the worktree's `target/`, and the lane script removed it.
- I took and released a worktree lease for session `adv2-w33-u4`.

**7. Findings block**
```findings
- file: crates/spindle-kernel/src/fit.rs
  line: 394
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: OneLine escapes only \n and \r, so a statement holding VT, FF, NEL, U+2028 or U+2029 (which ESS compiles) splits the misfit line, against its own doc and D2 A
- file: crates/spindle-kernel/src/fit.rs
  line: 394
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: ESC, BEL, BS, NUL and C1 controls in an invariant statement are written raw into the line run prints on stdout
- file: crates/spindle-kernel/src/fit.rs
  line: 394
  category: property
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the escape is not injective, so a statement holding a line break and one holding a written backslash-n print the same misfit line
```
