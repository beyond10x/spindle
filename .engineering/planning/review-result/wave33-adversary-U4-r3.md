---
format: aep.planning-md/3
id: review-result:wave33-adversary-U4-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 33 U4
relations:
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
unit: story:entity-invariant-over-stored-rows (wave 33 U4), round 3, covers head 96668864 with tests committed as 4ab6bf719
verdict: REFUTED (nothing found that holds the unit; one test gap closed, one doc note)
cases: executed 544→548, red 0
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 9 paths (part 6)
needs-coordinator: none

**1. Diff stat (96668864..4ab6bf719)**
```
 .../tests/spindle-kernel/adversary_w33_u4.rs       | 344 +++++++++++++++++++++
 1 file changed, 344 insertions(+)
```
Only a test file. No implementation file was touched.

**2. Cases added** to `<local path>`. All are kept and green now. Run alone, before the suite: `4 passed; 0 failed ... finished in 1.98s`.
- `adv3_every_text_of_every_misfit_is_written_by_one_reversible_escape`: a property test with a fixed seed. It draws about 3000 texts from backslash, `u`, braces, hex digits, `n`, `r`, every line break, C0/C1/DEL controls, tab, backtick, U+FEFF, U+202E, and characters of 2, 3 and 4 bytes, plus written `\u{1b}` and real ESC. Each of the 13 text fields of the 7 variants must print exactly as a separate reference escape writes it into the variant's template. A reference decoder must read each text back. No line may hold a control character other than tab, or U+2028/U+2029. No two texts may print the same line.
- `adv3_every_character_is_written_by_the_escape_and_reads_back`: every Unicode scalar value, U+0000 to U+10FFFF, goes through the escape and reads back.
- `adv3_an_invariant_over_the_identity_is_judged_as_at_rest_judges_it`: with the invariant `id != 'b1'`, `at_rest` refuses only b1, and the result is 1 invariant misfit.
- `adv3_an_invariant_over_the_state_is_judged_as_at_rest_judges_it`: with `state != 'Shelved'`, `at_rest` refuses all 3 books, and the result is 1 misfit with 3 rows.

Mutant probe, in my own managed tree (finished afterwards). The patch is `mutant.patch` in scratch: `Held::of` builds the row's facts with `Value::Null` in place of the identity, or with the row's state blanked. Each mutant ran the whole `spindle-kernel` suite: `532 passed; 1 failed`. The one failure each time was the new identity case or the new state case:
```
left: []
right: [Invariant { entity: "keep.shelf.Book", invariant: "id != 'b1'", rows: 1 }]
left: []
right: [Invariant { entity: "keep.shelf.Book", invariant: "state != 'Shelved'", rows: 3 }]
```
So the unit's tests and rounds 1–2 did not test that identity and state are read as `at_rest` reads them. The code does read them; these cases now cover it.

**3. Suite (run after the cases existed)**
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 0; `15 passed` (lib) and `533 passed; 0 failed; 71 ignored ... 7.33s`.
- The same with `-- --skip adversary_w33_u4::adv3`: exit 0; `529 passed; 0 failed; 71 ignored; 4 filtered out`. That gives the 544 "before" count.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-kernel --check`: exit 0, after formatting the test file with `slot --fetch … rustfmt --edition 2024`.

**4. Findings that are not failing cases**
- **Suite gap (mutant), `crates/spindle-kernel/src/fit.rs:552`.** The facts are built through `Facts::row`, which binds the identity and the state. Before this round no test noticed if either was dropped. Closed by the two cases above. Who reaches it: any entity invariant that reads `id` or `state`; ESS compiles both. CONFIRMED, introduced, note.
- **Docs do not mention the escape, `website/docs/concepts/environment-and-admission.md:701` and `CHANGELOG.md:36`.** Both say the line names "the statement", and `fit.rs:349` says "as its author wrote it". Since 96668864, a backslash in a statement prints doubled. For example, `title != 'a\nb'` with a written backslash prints `a\\nb`. Control characters print as `\u{..}`. This applies to every variant, so it also changes existing State lines whose state name holds a backslash. Neither the docs, `cli.rs` nor the CHANGELOG say so. Who reaches it: an ESS quoted literal keeps backslashes verbatim, and round 2's `one_misfit("\\\\n")` compiles one. Fix: one doc sentence and one CHANGELOG clause. CONFIRMED, introduced, note.

**5. Attacked and not broken**
- Injectivity: every backslash starts `\\`, `\n`, `\r` or `\u{hex}`, so the output decodes uniquely.
- Written `\u{1b}` text against a real ESC: they print apart.
- Tab stays raw in every field of every variant.
- The escape gives the same result however `write!` splits the text into pieces.
- Lowercase hex and multi-byte offsets (a mutant using `at + 1` would panic).
- Line forgery: two different misfits cannot print the same line, since entity names have no spaces and each variant's fixed text differs.
- The 10 acceptance tests exist and assert what the acceptance says, including record and row unchanged and stdin not read.
- The PostgreSQL acceptance case and round 2's PostgreSQL case are both picked up by the `check:postgres:run` filter.
- `Moved`/`Entity` rows are not judged by invariants. An entity the record lacks is judged.
- The status row, the `cli.rs` text and the generated `cli.md` match the code.

**6. Paths written outside the worktree**
- `<local path>` (directory) holding `adv3-alone.log`, `kernel-suite.log`, `kernel-before.log`, `fmt.log`, `clippy.log`, `mutant.patch`, `mutant-identity.log`, `mutant-state.log`, `commit-msg.txt`.
- Managed worktree `<local path>` (id `spindle-w33-u4-adv3-mutant`). It is clean, its cache was discarded and it is marked finished. I did not run gc on it.

**7. Findings**
```findings
- file: crates/spindle-kernel/src/fit.rs
  line: 552
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: building the row's facts without the identity or without the state left every unit and round 1-2 case green; adv3 identity and state cases now catch both mutants
- file: website/docs/concepts/environment-and-admission.md
  line: 701
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: docs, cli.rs and CHANGELOG say the misfit line names the statement as written, but since 96668864 every misfit text prints escaped (backslash doubled, controls as \u{..}) and no document says so
```
