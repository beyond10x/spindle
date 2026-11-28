---
format: aep.planning-md/3
id: review-result:wave31-adversary-U5-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 31 U5
relations:
- reviews: story:spec-change-over-stored-rows
revision: 1
---
```
unit: story:spec-change-over-stored-rows (wave 31 U5), round 2 against the correction at 3c814a3b; tests committed as cf566826 on unit/w31-spec-change
verdict: CONFIRMED
cases: executed spindle-kernel 503→515, spindle-cli 644→645, red 1 (plus the 3 known UI reds)
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 11 more files in U5/adv-scratch (part 6)
needs-coordinator: none
```

The correction holds on everything you listed: both round-1 reds now pass, and the nested types, the optional enum, a null under a field made required, the moved domain, and loosened, tightened and unchanged constraints all behave as the story says. One new red: a struct that used to accept undeclared members and is closed again is still accepted, although stored values hold such members. That is a note-level edge, not a blocker. I ran no mutants this round.

**1. Diff stat** (`git --no-pager diff --stat 3c814a3b HEAD`). Every path is a test or a fixture.
```
 .../adversary-w31-u5-moved-after/domains/shelf.yaml   |  27 ++
 .../adversary-w31-u5-moved-after/domains/vault.yaml   |  27 ++
 .../adversary-w31-u5-moved-after/system.yaml          |  32 +++
 .../adversary-w31-u5-moved-before/domains/shelf.yaml  |  27 ++
 .../adversary-w31-u5-moved-before/domains/vault.yaml  |  27 ++
 .../adversary-w31-u5-moved-before/system.yaml         |  32 +++
 crates/spindle-cli/tests/spindle-cli/adversary_w31_u5.rs     | 130 +++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs                 |   1 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w31_u5.rs | 316 ++++++++++
```

**2. Cases added.** All run in well under 1 s.

Kernel (`r2_*` in `adversary_w31_u5`):
- **Nested types:**
  - an enum widened inside a struct inside a list, and inside a map, fits;
  - narrowed where a nested value uses the removed value, it misfits as `ratings` and `tags`;
  - narrowed where no stored value uses it, it fits.
- **Optional enum narrowed:** only the stored value misfits, not the `null`.
- **Null under a field made required:** reported only as `Required` (rows 2), with no `Retyped`.
- **Made required and the enum widened together:** fits.
- **Struct member removed** while a stored tag holds it: misfits.
- **Newtype constraints:**
  - prefix removed and invariant loosened: fits;
  - invariant tightened that no stored value breaks: fits;
  - prefix tightened that a stored value breaks: misfits;
  - an unchanged constrained type next to another changed field: fits.

  All of these are green.
- **`r2_a_struct_closed_to_undeclared_members_misfits_where_a_stored_value_holds_one`: RED.**

CLI: `a_domain_moved_between_served_components_exits_3_naming_the_new_owner`, with the two new fixtures: green. It is a served run of two components on `laptop.toml` with SQLite files. The book's domain moves from `archive` to `vault` and its title is also retyped. The run exits 3 and stdout names `archive.shelf.Book` and "moved to component vault". It prints neither "removed" nor a listening line, and the file and its record are unchanged.

The first run of the kernel cases did not compile its own YAML: the unquoted `Map<String, ...>` breaks the flow mapping. I quoted it before any assertion ran. Red output of the case run alone:
```
panicked at crates/spindle-kernel/tests/spindle-kernel/adversary_w31_u5.rs:586:5:
  left: Fits { unrecorded: false }
 right: Misfits([Retyped { entity: "keep.shelf.Book", field: "tags", rows: 1 }])
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 541 filtered out; finished in 0.04s
```

**3. Suites**, run after the cases existed:
- `cargo test --locked -p spindle-kernel --no-fail-fast`, exit 101:
  - lib: `15 passed`;
  - integration: `499 passed; 1 failed; 53 ignored`. The only failure is the red above, and both round-1 cases now pass.
- `cargo test --locked -p spindle-cli --no-fail-fast`, exit 101: `637 passed; 3 failed; 34 ignored`. The 3 failures are the known UI cases (`deploy::the_ui_is_served_from_the_binary` and the two `observe_panel::` cases).
- `cargo fmt --check` passes on both crates.

**4. Finding**
- **`crates/spindle-kernel/src/fit.rs:530`** (the struct arm of `structure`, line 497), category acceptance, severity note, CONFIRMED, introduced.
  - **Measured:** a struct that had `undeclared_fields: ignored` is closed again, and the store keeps a tag holding an undeclared member. The struct's structure leaves the setting out, so the two structures are equal and nothing is read again. The store fits, while `typed::validate` (the correction's own test of whether a value reads) refuses that value.
  - **What reaches it:** a command input of an open struct keeps its undeclared members. `typed::integers` copies them through (`typed.rs:85`), `execute.rs:306` stores them, and a `sets:` writes them to the row. I did not drive that end to end; the case commits the row directly.
  - **Fix:** write `ir.undeclared_fields(name)` into the struct's structure.

**5. Attacked and could not break**
- **Fallback:** it is reached only by shapes built by hand. Every real caller uses `Shapes::of` or `Shapes::owned` as the current side, so the new model is always the side that reads; the recorded side never carries one.
- **Moved and retyped together:** reports `Moved` alone.
- **Moved message:** goes through the host's `HostError::Misfit`, giving exit 3 with the text on stdout.
- **PostgreSQL vs SQLite:** they agree by construction, since `misfits` is generic over `Store`. I did not run the PostgreSQL lane this round.
- **Unchanged specification:** short-circuits at `Same`.
- **Invariants compared by their written statement:** safe, because ESS parses the predicate from that text.

**6. Written outside the worktree**, all in `<local path>`:
- `round2-kernel.rs`
- `r2-alone.sh`, `r2-alone.out`, `r2-kernel-alone.log`, `r2-cli-alone.log`
- `r2-suites.sh`, `r2-suites.out`, `r2-suite-kernel.log`, `r2-suite-cli.log`
- `commit-msg-2.txt`

Round 1's files are still there. I created no new worktree.

```findings
- file: crates/spindle-kernel/src/fit.rs
  line: 530
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a struct closed again to undeclared members fits stored values holding one, because the struct structure omits undeclared_fields and no value is read again, though typed::validate refuses it
```
