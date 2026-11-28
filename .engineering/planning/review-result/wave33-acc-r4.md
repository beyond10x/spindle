---
format: aep.planning-md/3
id: review-result:wave33-acc-r4
kind: review-result
status: active
title: Plan critic (acceptance), round 4, on wave 33
relations:
- reviews: story:listener-limits
- reviews: story:new-leader-write-while-old-paused
- reviews: story:seed-record-after-a-component-is-removed
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
approve

**What I read:** all 4 stories, each in full with `aep plan artifact show` and `cat -n`: listener-limits, new-leader-write-while-old-paused, seed-record-after-a-component-is-removed and entity-invariant-over-stored-rows. I also read the wave page, `review-result:wave33-acc-r1`, `-r2`, `-r3` and `wave33-par-r2`, and `git diff 7b8f85a10 195456ffd`. In the tree at 195456ff I checked `adversary_w32_u4.rs` (the part-done record built by SQL, the three-table case at `:228`), `seed_file` (`replicas.rs:880`, three independent `OpenTask` entries), the `Lease` functions in kernel `postgres.rs`, and `adversary_w31_u6.rs:1855-1896`. `aep plan artifact validate` prints `valid`.

**Round-3 findings are both fixed:**
- `a_record_in_a_current_table_wins_over_an_orphan_record` (`seed-record-after-a-component-is-removed.md:95-103`): `audit`'s record is at `done` 1 of 3 and `board`'s at `done` 2, so release 2 sends only entry 3. No release stored that entry, so the resend that was rejected as `undeclared` is gone. Under D2 A the run passes; under D2 B the two records differ and it exits 1.
- `a_move_out_of_an_orphan_table_whose_lease_another_holds_is_refused` (`:127-132`) is new. Another handle holds the orphan table's lease, so under D1 A the move is refused naming that lease. Under D1 B the move would succeed.

**What I could not establish:**
- **Not run:** I read every cited path in the code and ran none of them.
- **Test setup (an unease, not a finding):** the fixed case says release 1 "starts with a 3-entry seed file and is stopped" at `done` 1 of 3 (`seed-record-after-a-component-is-removed.md:95-96`). Read literally, that means stopping a run partway, which depends on timing. The neighbouring tests in `adversary_w32_u4.rs:95-121` build the same state without timing: they seed one entry, then set the record with SQL. The line's "as the first case leaves it" points to that method, so I did not raise it.
- **Test budget:**
  - Every new PostgreSQL-lane test has a stated time. U2 gives 15 s, 5 s and under 30 s for its rewritten case; each of U3's tests is expected under 10 s, none timed yet; U4 gives under 5 s.
  - Out of my lane (scope or parallel safety): the wave page still says U3 adds "four kernel tests" and gives U3 80 s, about 105 s for the wave (`wave-33.md:158-162`). With the new test U3 has five kernel tests, which makes U3 about 90 s and the wave about 115 s.
  - Also out of my lane: the story's Scope list of kernel tests (`seed-record-after-a-component-is-removed.md:170`) does not name the new test.
- **Public repository and test rules:**
  - No story body names a person, organisation, host or home path.
  - None adds Python, and none reaches a network beyond loopback.
  - Every PostgreSQL test names `task check:postgres`.

```findings
[]
```
