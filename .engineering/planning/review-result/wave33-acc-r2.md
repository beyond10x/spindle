---
format: aep.planning-md/3
id: review-result:wave33-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 33
relations:
- reviews: story:listener-limits
- reviews: story:new-leader-write-while-old-paused
- reviews: story:seed-record-after-a-component-is-removed
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
needs-revision

story:entity-invariant-over-stored-rows — no acceptance line checks D1 A ("every invariant the new model declares … the record format is unchanged, so a row that already broke an unchanged invariant now refuses the next start"): every listed test adds the invariant only in the new fixture, so D1 B (check only invariants missing from the recorded shape) passes them all; the fix is a line where both models declare the invariant and a stored row breaks it, or a line saying the recorded shape still holds only `fields` and `states` — .engineering/planning/story/entity-invariant-over-stored-rows.md:103

story:entity-invariant-over-stored-rows — no acceptance line checks D2 A ("one line per broken invariant"): every test breaks a single invariant on a single entity, so one line per entity (D2 B) gives the same output; the fix is a line where one row breaks two invariants of `archive.shelf.Book` and the output has two lines, each naming its statement — .engineering/planning/story/entity-invariant-over-stored-rows.md:108

What I read: all 4 of the 4 ids, each body in full with `aep plan artifact show`. I also read the wave page, `review-result:wave33-acc-r1` and `story:postgres-lane-margin`. In the tree at c31c23b7 I checked:
- the `Desk` and `HOLD` (2 s) in `crates/spindle-cli/tests/spindle-cli/concurrent_reads.rs:34-62`
- the `spindle_kernel::postgres_prefix` re-export (`crates/spindle-cli/src/database.rs:28`)
- the model test at `crates/spindle-kernel/tests/spindle-kernel/postgres.rs:937`
- how `adversary_w32_u4.rs` writes part-done records with SQL
- that `spec_change.rs` checks misfits on stdout
- that `Shape` holds only `fields` and `states` (`fit.rs:79-85`)
- the resumed-leader case at `adversary_w31_u6.rs:379-420`

All three round-1 acceptance findings are fixed:
- `run_flags_bound_the_listener` now has a 12 KiB head, a 500 ms / 2 s cap check and a held-desk `504`, and each check fails on the defaults.
- The cap test now names how long it waits.
- `a_record_in_a_current_table_wins_over_an_orphan_record` now tells D2 A from D2 B.

What I could not establish:
- story:listener-limits, `a_head_over_the_limit_answers_431`: the item defines "Writes nothing" (`listener-limits.md:84`), but neither the item nor the Outcome says it. One can read the test as asserting that no exchange or request record is left, or as not asserting it. This is wording, not a finding.
- story:seed-record-after-a-component-is-removed, `a_record_in_a_current_table_wins_over_an_orphan_record`: release 1 completed the seed (done 1 of 1). Release 2 then sends entry 1 again from `board`'s done-0 record. I did not check whether that resend can succeed against the rows already stored.
- Test budget: none of the new or rewritten tests adds more than about 10 s to the PostgreSQL lane without saying so.
  - U2: 15 s and 5 s for its new tests, and "under 30 s" for the rewritten case with its retry of up to 10 s.
  - U3: "expected under 10 s" for each of its eight tests, none timed yet.
  - U4: under 5 s.
  - The wave page sums these to about 105 s and has `story:postgres-lane-margin` merge first. The current time of `a_resumed_leader_posts_no_event_the_new_leader_committed` is not recorded, so how much its retry adds is not known.
- Public repository: none of the four story bodies names a person, organisation, host or home path. None adds Python or reaches a network beyond loopback. Every PostgreSQL test names `task check:postgres`.
- Out of my lane: none seen.

```findings
[{"file": ".engineering/planning/story/entity-invariant-over-stored-rows.md", "line": 103, "category": "acceptance", "severity": "warning", "verdict": "needs-revision", "origin": "introduced", "message": "no acceptance line checks D1 A (every declared invariant, record format unchanged, a row breaking an unchanged invariant refuses the next start): every listed test adds the invariant only in the new fixture, so D1 B passes them all; add a line where both models declare the invariant and a stored row breaks it, or one that the recorded shape still holds only fields and states"},
 {"file": ".engineering/planning/story/entity-invariant-over-stored-rows.md", "line": 108, "category": "acceptance", "severity": "warning", "verdict": "needs-revision", "origin": "introduced", "message": "no acceptance line checks D2 A (one line per broken invariant): every test breaks a single invariant on a single entity, so one line per entity (D2 B) gives the same output; add a line where one row breaks two invariants of archive.shelf.Book and the output has two lines, each naming its statement"}]
```
