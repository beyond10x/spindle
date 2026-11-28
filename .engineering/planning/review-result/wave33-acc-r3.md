---
format: aep.planning-md/3
id: review-result:wave33-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 33
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
needs-revision

story:seed-record-after-a-component-is-removed — the acceptance for `a_record_in_a_current_table_wins_over_an_orphan_record` cannot pass, and it does not tell D2 A from D2 B. `board`'s planted record says `done` 0, so release 2 sends entry 1 again. That is `tasks.board.OpenTask` for `s1`, which release 1 already stored. `OpenTask` declares no `existing_instance:` (`crates/spindle-cli/tests/fixtures/ingress/domains/board.yaml:31-46`). So the command fails as `undeclared` ("a `…` already has the identity …, and `creates:` never replaces an instance", `crates/spindle-kernel/src/execute.rs:2126-2133`). The seed counts that as `Rejected` (`crates/spindle-cli/src/seed.rs:982`), and a rejected entry stops the run with exit 1 (`seed.rs:833-835`, `serve.rs:688`). Under D2 A the run therefore exits 1 just as it would under D2 B. The fix: plant a record that only sends entries nothing has stored yet. For example, release 1 seeds `s1` and its record names a 3-entry file at `done` 1, `board`'s record names the same file at `done` 2, and release 2 sends entry 3 only. — .engineering/planning/story/seed-record-after-a-component-is-removed.md:96-100

story:seed-record-after-a-component-is-removed — no acceptance line checks D1 A's guard on the orphan table's own lease during the move:
- In `an_orphan_record_under_another_holders_lease_is_read_not_moved`, no move is ever attempted, so "neither table changes" is true by default.
- In `a_deposed_process_cannot_move_the_seed_record_out_of_a_removed_components_table`, only the first component's lease is taken away.
- So a gather guarded by the first component's lease alone (D1 B) passes both tests.

The fix: one line where another handle holds the orphan table's lease, A attempts the move, the move is refused naming that table's lease, and both tables are unchanged. — .engineering/planning/story/seed-record-after-a-component-is-removed.md:117-124

**What I read:** all 4 of the 4 ids, each body in full with `aep plan artifact show`. I also read the wave page, `review-result:wave33-acc-r1` and `wave33-acc-r2`, and `git diff c31c23b71 7b8f85a10`. In the tree at 7b8f85a1 I checked:
- `fit.rs`: `Shapes` with `model` under `#[serde(skip)]`, `Shape`, and `misfits`
- `seed.rs`: its module doc, the `Became` classification and `failed`
- `execute.rs`: `creation_identity`
- the `board.yaml` fixture
- `replicas.rs`: `seed_file`
- `adversary_w32_u4.rs` and the model kernel test at `postgres.rs:937`
- `concurrent_reads.rs`: `Desk`, `HOLD`
- `adversary_w30_u5.rs:205`, where a duplicate creation fails as `undeclared`

**Round-2 findings:**
- Both round-2 findings on story:entity-invariant-over-stored-rows are fixed:
  - `an_invariant_both_models_declare_is_judged_over_the_stored_rows` fails under D1 B, and its serialisation assertion fixes the record format.
  - `a_row_breaking_two_invariants_misfits_once_per_invariant` fails under D2 B.
- Round 2's open question (can release 2's resend of entry 1 succeed?) is now settled. It cannot, which is the first finding above.

**What I could not establish:**
- **Not run:** I did not run any of the cited code paths. The resend rejection is read from the code and from the `adversary_w30_u5.rs` case, not from a run of this test.
- **Test budget:** no wave-33 test adds more than about 10 s to the PostgreSQL lane without saying so. U2 states 15 s, 5 s and "under 30 s" for its rewritten case. U3 states "expected under 10 s" for each of its eight tests, none of them timed yet. U4 states under 5 s. The wave page sums them to about 105 s and has the margin story merge first.
- **Public repository and test rules:** no story body names a person, organisation, host or home path, and none adds Python. Nothing reaches a network beyond loopback. Every PostgreSQL test names `task check:postgres`.
- **Out of my lane:** none seen.

```findings
[{"file": ".engineering/planning/story/seed-record-after-a-component-is-removed.md", "line": 96, "category": "acceptance", "severity": "blocker", "verdict": "needs-revision", "origin": "introduced", "message": "the acceptance for a_record_in_a_current_table_wins_over_an_orphan_record cannot pass and does not tell D2 A from D2 B: board's planted record at done 0 makes release 2 send entry 1 (tasks.board.OpenTask s1) again onto the task release 1 stored; OpenTask declares no existing_instance, so the command fails as undeclared, the seed counts it Rejected and the run exits 1 under D2 A as under D2 B; plant records that send only entries nothing has stored yet (e.g. a 3-entry file, audit at done 1, board at done 2, release 2 sends entry 3 only)"},
 {"file": ".engineering/planning/story/seed-record-after-a-component-is-removed.md", "line": 122, "category": "acceptance", "severity": "warning", "verdict": "needs-revision", "origin": "introduced", "message": "no acceptance line checks D1 A's guard on the orphan table's own lease during the move: an_orphan_record_under_another_holders_lease_is_read_not_moved never attempts a move, so neither table changes by default, and the deposed-process case takes away only the first component's lease, so a gather guarded by the first component's lease alone (D1 B) passes both; add a line where another handle holds the orphan table's lease, A attempts the move, the move is refused naming that table's lease, and both tables are unchanged"}]
```
