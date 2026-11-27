---
format: aep.planning-md/3
id: review-result:wave31-adversary-U4-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 31 U4
relations:
- reviews: story:verified-caller-token
revision: 1
---
unit: story:verified-caller-token, round 3, findings cover head 7bb89acc (the correction 2f697e1f plus my test commit)
verdict: nothing found
cases: executed 681→684, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths (listed in part 6)
needs-coordinator: the managed tree `spindle-w31-u4-adv-mutants` from round 1 is still kept by `worktree gc` ("no-remote-recovery-proof"), so removing it is yours.

**Summary.** I could not break G1 or G3. Round 2's red case, `a_token_whose_caller_does_not_bind_reads_none_of_the_runs_own_routes`, is now green. The only failures in the suite are the 3 UI cases already red on the base.

## 1. `git --no-pager diff --stat 2f697e1f HEAD`
```
 .../tests/spindle-cli/adversary_w31_u4.rs          | 251 ++++++++++++++++++++-
 1 file changed, 250 insertions(+), 1 deletion(-)
```
It is a test file only; no implementation file is touched. The 1 deletion is in my own helper: `serve` now calls a new `serve_on(spec, …)`. Commit 7bb89acc on `unit/w31-token`, on top of 2f697e1f. The privacy grep printed nothing.

## 2. Cases added, all green
They use the existing fixture `adversary-fixed-caller`: a Librarian with `branch` (enum), `level` (Integer) and `nickname` (`Optional<String>`), and a Visitor with no attributes.

- **`an_optional_attribute_left_unmapped_starts_and_its_token_binds`:** with `nickname` not mapped, the run starts.
  - A librarian token reads the view and gets 200 on requests, metrics and topology; ShelveBook is not answered 403.
  - A visitor token with unrelated claims beside its role binds.
- **`a_librarian_token_whose_attributes_do_not_bind_is_no_caller`:** eight token shapes each get 403 `{"refused":"not granted","actor":null}` on requests, metrics, topology and the view:
  - `level` as `"3"`, as `3.5`, or `null`
  - `branch` as `"East"` or `"north"`
  - no `branch`, or no `level`
  - a mapped optional `nick` set to `5`
  
  Two role lists (`["librarian","visitor"]` and `["visitor"]`) get 403. A librarian that does not bind gets 403 and is not read as the visitor it would bind as. `recorded` is 8: the views only, nothing under `/__spindle/`.
- **`a_required_attribute_mapped_from_no_claim_exits_1_and_an_unmapped_actor_does_not`:** these exit 1 with a line naming `--caller-token`, the file, `shelf.book.Librarian` and the missing attribute:
  - `level` left out
  - only `nickname` mapped
  - a second role `head` mapped to Librarian, with `level` left out
  
  A mapping that maps only the visitor starts.

Before I fixed it, the third case failed because of a bug in my own test: I had left `--observe` off its last run, so `/__spindle/requests` answered 404. After the fix the module ran `15 tests run: 15 passed`.

## 3. Suite, run once after the cases existed
- `SLOT_TIMEOUT=30m cargo test --locked -p spindle-cli --no-fail-fast` → exit 101
  ```
  test result: ok. 8 passed; 0 failed; ...
  test result: FAILED. 673 passed; 3 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.39s
  ```
- The 3 failures are `deploy::the_ui_is_served_from_the_binary` and the two `observe_panel::` cases, already red on the base.
- The "before" count of 681 is derived from this same run, minus my 3 round-3 cases.
- clippy `-D warnings` → exit 0. fmt `--check` → exit 0.

## 4. Findings
None.

## 5. Attacked and not broken
- An optional attribute left unmapped, or mapped from a claim the token leaves out: the run starts and the token binds.
- A wrong-typed required or optional attribute, null, an enum value outside the variants or in the wrong case: no caller, on the run's own routes and on the view.
- A role list, and a librarian that does not bind being taken as the visitor: no caller in both cases, with no fallback to another actor.
- A caller that binds still reaches requests, metrics and topology.
- Nothing under `/__spindle/` is recorded.
- G3 required-attribute check: a second role on the same actor is caught, and an actor no role maps needs no attributes.

## 6. Paths written outside the worktree
All under `<local path>`:
- `r3-append.rs`
- `r3-module.log`
- `r3-suite.log`
- `r3-clippy.log`
- `commit-msg-r3.txt`
- the directory itself, which still holds the round 1 and 2 files

## 7.
```findings
[]
```
