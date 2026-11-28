---
format: aep.planning-md/3
id: review-result:wave33-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 33 U3
relations:
- reviews: story:seed-record-after-a-component-is-removed
revision: 1
---
unit: U3, story:seed-record-after-a-component-is-removed. Findings cover head 1fa6832c6: the unit's 114cd03ca plus my test commit.
verdict: CONFIRMED
cases: executed 184→186, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths, listed in part 6
needs-coordinator: the base worktree I created (`spindle-w33-u3-adv-base`) is marked finished but not removed; `worktree gc` removes it.

**Summary:** one red case. A reset can leave a removed component's seed record in place. The next seeded start then reads that record and sends nothing to the emptied store. The same case passes on the base.

Your question was whether an old-release process can later write a different record into the orphan table that a later start then reads. While the first component's table holds a record, it cannot: the next start reads the first table's record, and a green test proves it. After a reset that left the orphan record, a later start does read it: that is the red case.

### 1. `git --no-pager diff --stat 114cd03ca..HEAD`
```
 .../tests/spindle-cli/postgres/replicas.rs         |   1 +
 .../postgres/replicas/adversary_w33_u3.rs          | 208 +++++++++++++++++++++
 2 files changed, 209 insertions(+)
```
Both paths are test files. The `replicas.rs` change is only the `mod adversary_w33_u3;` line.

### 2. Cases added
File: `<local path>`. Both are PostgreSQL-lane cases. The old-release process is a `spindle_kernel::Lease` the test holds on `audit`, the way an old-release standby holds it during a rollout: it takes leases in name order, `audit` first, then stands by on `board`.

**`a_reset_while_an_old_release_holds_the_removed_components_lease_leaves_the_seed_to_be_sent_again`: red, 1.8 s**
- Release 1 seeds `[s1, s2]` and its record goes into `audit`'s table.
- The old process takes `audit`'s lease.
- Release 2 without a seed runs `POST /api/reset` and gets 200.
- The old process releases the lease.
- Release 2 starts with the same seed. The test asserts that `GET /tasks` is `[s1, s2]`.

Red output from the first run of this case alone:
```
panicked at crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w33_u3.rs:134:5:
assertion `left == right` failed: after the reset the whole seed is sent to the emptied store; audit's lease table still holds None.
reset run printed:
b10x-spindle: the seed record in spindle_test_mbyptybznhcn.spindle_audit_p_lease, the lease table of a component this specification no longer has, is left there: another process holds that table's lease
next start printed:
b10x-spindle: seed …/seed.json (sha256:0377bcc0…) already ran on the PostgreSQL store, as the row `seed` of spindle_test_mbyptybznhcn.spindle_board_p_lease records: no entry is sent
  left: []
 right: ["s1", "s2"]
```
At the end `audit`'s table holds no record, because the next start moved the stale record into `board`'s table and then skipped the seed. The panic line moved to :139 after formatting.

**`a_record_left_under_an_old_release_lease_is_written_to_the_first_table_and_wins_after`: green, 1.8 to 2.3 s**
- Release 1 leaves `audit`'s record at done 1 of 3, and the old process holds `audit`'s lease.
- Release 2 sends entries 2 and 3 and prints a line naming `audit`'s table and "another process holds".
- Afterwards `board`'s record is at done 3 and `audit`'s is unchanged.
- The old process then writes a different record (done 0) into `audit` under its lease. The next release-2 start prints "already ran", sends no entry, and leaves both records as they were.

This case covers the fallback path when the lease is held. No unit test reached that path, so a mutant that drops `record_seed` in that path would have stayed green.

### 3. Suite run, after the cases existed
- Cases alone: `slot --timeout 30m --tokens 4 sh scripts/postgres-lane.sh lane -- cargo nextest run --locked --profile postgres -p spindle-cli --test spindle-cli --run-ignored all -E 'test(~adversary_w33_u3::)'` gave `2 tests run: 1 passed, 1 failed, 833 skipped`, exit 100.
- Base comparison in a managed worktree at 3a5ec126d, with the module copied in: the red case gave `1 test run: 1 passed, 829 skipped`, exit 0. So the finding is introduced by the unit.
- Whole lane: `slot --timeout 30m --tokens 4 task check:postgres` gave `Summary [41.250s] 186 tests run: 185 passed, 1 failed, 1510 skipped`, `within: 41.724 s of 100 s`, exit 201. The one failure is my red case. The lone timing case did not run because the parallel part failed first.
- The "184" before-count is the same run's 186 minus my two cases. No separate run was made to get it.
- `cargo fmt -p spindle-cli -- --check`: exit 0. `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.

### 4. Findings
**A reset leaves the orphan record while its lease is held** (CONFIRMED, introduced, warning)
- **What was measured:** `adversary_w33_u3.rs:139`. In `crates/spindle-cli/src/database.rs:658` (`forget_orphans`), a held orphan lease only prints a warning and `continue`s, and the reset still answers 200. `orphaned()` then reads that record on the next seeded start, which sends no entry to the emptied store. That breaks `story:seed-under-a-lease` D6 A: a start with a seed after a reset should treat the store as having no record. The implementor's own reset test names exactly this danger.
- **What reaches it:** a rolling deploy from a release that had the removed component, where old-release standbys still hold its lease (taken first in name order), and a `POST /api/reset` on a leased release-2 run without a seed during that window. The concepts page documents "keeps its row, and a line says so", but not the skipped seed that follows.
- **Fix:** do not delete the current record on a reset. Instead write into the first component's table a record that marks nothing done, so the current table wins under D2 A and the orphan is never read. Alternatively, refuse the reset (non-200) while an orphan lease is held.

### 5. Attacked, not broken
- **LIKE pattern:** the `_` escaping and the lowercase-name filter are correct, and every `prefix` output is lowercase letters and `_`, hashed ones included.
- **Taking an orphan lease:** it never inserts a lease row (`TAKE_EXISTING` is an UPDATE). A held, expired or dead-session lease each behaves as intended.
- **Move under two leases:** a deposed mover is refused, covered by the unit's kernel tests.
- **Your question:** an old-release process writing a different record into the orphan table does not reach a later start while the first table holds a record (my green case). It does reach one after a reset that left the orphan record (my red case).
- **Hosted serving role:** I ruled out a permission failure on orphan tables by reading the docs, not by a test: the documented grant is `ON ALL TABLES IN SCHEMA`, which covers them.

### 6. Paths written outside the worktree
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`: the managed base worktree, made with `worktree create`. My test edits there were reverted, and it is marked finished.

### 7. findings
```findings
- file: crates/spindle-cli/src/database.rs
  line: 658
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a reset while another process holds a removed component's lease answers 200 and leaves that table's seed record, which the next seeded start reads to skip the whole seed on the emptied store (red at crates/spindle-cli/tests/spindle-cli/postgres/replicas/adversary_w33_u3.rs:139, green on the base)
```
