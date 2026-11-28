---
format: aep.planning-md/3
id: review-result:wave31-adversary-U4-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 31 U4
relations:
- reviews: story:verified-caller-token
revision: 1
---
unit: story:verified-caller-token, round 2, findings cover head ba207a66 (the correction 94a445f2 plus my test commit)
verdict: CONFIRMED
cases: executed 677→681, red 1
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths (listed in part 6)
needs-coordinator: the managed tree `spindle-w31-u4-adv-mutants` from round 1 is still kept by `worktree gc` ("no-remote-recovery-proof"), so removing it is yours.

**Summary.** F1 and F3 hold. F2 has one gap: a token whose role maps to an actor but whose claims don't make a valid caller still reads the request log, the metrics and the topology. A command answers that same token 403 with no actor. Round 1's red case, `a_key_removed_by_emptying_the_set_no_longer_verifies`, is now green.

## 1. `git --no-pager diff --stat 94a445f2 HEAD`
```
 .../tests/spindle-cli/adversary_w31_u4.rs          | 278 +++++++++++++++++++++
 1 file changed, 278 insertions(+)
```
It is a test file only; no implementation file is touched. Commit ba207a66 on `unit/w31-token`, on top of 94a445f2. The privacy grep printed nothing.

## 2. Cases added
All are in `<local path>`.

**Red:** `a_token_whose_caller_does_not_bind_reads_none_of_the_runs_own_routes`. Two member tokens: one with no `sub`, one with `sub: 7`. A command answers both 403 `{"refused":"not granted","actor":null}`. The test then expects `/__spindle/requests`, `/metrics` and `/topology` to answer 403 as well. Run alone first:
```
panicked at crates/spindle-cli/tests/spindle-cli/adversary_w31_u4.rs:1064:13:
assertion `left == right` failed: no sub: /__spindle/requests answers a token of no caller: {"capacity":256,"format":"spindle-requests/1","recorded":1,"requests":[{... "operation":"porch.visit.RecordVisit", "status":403 ...}], ...}
  left: 200
 right: 403
test result: FAILED. 0 passed; 1 failed; ... 705 filtered out; finished in 0.02s
```

**Green, kept:**
- `a_key_set_that_empties_breaks_and_refills_revokes_and_restores_its_keys` (5.1 s, over 5 s), one step at a time:
  - The set goes empty: the key gets 401, and a "was refreshed" warning names the URL.
  - Then the server sends HTML: a warning, and still 401, so the empty set is kept.
  - Then the old key comes back: 200 again.
  - Then `{"keys":"none"}`: a warning, and still 200, so the usable set is kept.
  - Then kid `k1` reappears with different key material: the new key gets 200 and the old one gets 401.
- `the_runs_own_routes_refuse_a_bad_token_401_before_an_unmapped_one_403`, run with `--observe`. On requests, metrics, topology and other:
  - An unmapped claim signed by a foreign key gets 401, and so does an expired unmapped claim; a valid unmapped token gets 403 `actor: null`.
  - live, ready and seed answer 200 whatever token is sent.
  - The unmapped token on a command gets 403.
  - Auditor and member tokens still get 200 on requests, metrics and topology.
  - The log shows `recorded: 1`: only the command's 403 is recorded.
- `a_mapping_naming_another_actors_attribute_or_an_unqualified_actor_exits_1`: each of these exits 1 with a line naming `--caller-token`, the file and the name:
  - `person_id` mapped for `porch.visit.Auditor`
  - `Member` with no domain
  - `porch.visit.member` in the wrong case
  - the domain `porch.visit` used as an actor

## 3. Suite, run after the cases existed
- `SLOT_TIMEOUT=30m cargo test --locked -p spindle-cli --no-fail-fast` → exit 101
  ```
  test result: ok. 8 passed; 0 failed; ...
  test result: FAILED. 669 passed; 4 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.24s
  ```
- The 4 failures are the 3 UI cases already red on the base plus my red case.
- The "before" count of 677 is derived from this same run, minus my 4 round-2 cases; I made no separate run.
- `cargo nextest ... -E 'test(/^adversary_w31_u4::/)'`: `12 tests run: 11 passed, 1 failed`.
- clippy `-D warnings` → exit 0. fmt `--check` → exit 0.

## 4. Findings
| # | file:line | verdict | origin | what was measured | what reaches it |
|---|---|---|---|---|---|
| 1 | crates/spindle-ingress/src/listener.rs:691 | CONFIRMED | introduced | `unmapped` only covers a token that maps to no actor at all. A token whose role maps but whose caller does not bind is treated as no caller by a command, yet it reads `/__spindle/requests`, metrics and topology (the red case, line 1064). | Any member token without `sub`, or with a bad one. It also covers every token of a role whose required attributes the mapping never maps, because F3 checks only names the specification doesn't declare, not required attributes left out (token.rs:319). Possible fix: check the bind (`Caller::binds`) before the reserved-route gate, or make `mapped` return `None` for a caller that does not bind. |
| 2 | crates/spindle-ingress/src/listener.rs:770 | INFEASIBLE | introduced | F2's recorded 403 on console routes (`unmapped` in the `Asked::Console` branch) cannot be reached: a token run never serves the console, and `--console on` exits 2. | Nothing found. It is dead code today, and no test can cover it. |
| 3 | crates/spindle-cli/src/token.rs:319 | CONFIRMED | introduced | `Mapping::check` refuses undeclared actors and attributes. A mapped actor with a required attribute missing from the mapping (Member with no `[attributes]` table) still starts the run. | An operator who forgets the attribute table: every such token then gets 403 on commands and, because of finding 1, reads the run's own routes. |

## 5. Attacked and not broken
- F1, a set that flips usable → empty → usable: keys are revoked and restored each time.
- F1, an unparseable body after an empty set: the empty set is kept.
- F1, a kid that reappears: only the new key material verifies.
- F1, the start of a run: an empty set exits 1 (round-1 case).
- F2 order: a bad token gets 401 before an unmapped token gets 403, and the probes are open to any token.
- F2 under `--observe`: nothing under `/__spindle/` is recorded, and the command's 403 is recorded.
- F2, mapped tokens: they still reach requests, metrics and topology.
- F3: another actor's attribute, an unqualified actor, the wrong case, or a domain named as an actor all exit 1 with the name.

## 6. Paths written outside the worktree
- Under `<local path>`:
  - `r2-src.diff`
  - `r2-append.rs`
  - `r2-red-alone.log`
  - `r2-module.log`
  - `r2-suite.log`
  - `r2-clippy.log`
  - `commit-msg-r2.txt`
- Round 1's scratch files and the mutant tree are still there.

## 7.
```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 691
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a token whose role maps but whose caller does not bind is no caller to a command (403, actor null) yet still reads /__spindle/requests, metrics and topology, so F2 covers only an unmapped role
- file: crates/spindle-ingress/src/listener.rs
  line: 770
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the recorded console 403 for an unmapped token cannot be reached, since a token run never serves the console and --console on exits 2
- file: crates/spindle-cli/src/token.rs
  line: 319
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the start check refuses only undeclared actors and attributes, not a mapped actor whose required attributes the mapping leaves out, so every such token is a caller that never binds
```
