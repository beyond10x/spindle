---
format: aep.planning-md/3
id: story:postgres-lane-margin
kind: story
status: implemented
title: The PostgreSQL lane ends within 90 s on the CI runner
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T14:03:08Z", actor: "human:operator", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-28T14:03:09Z", actor: "human:operator", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-28T14:03:09Z", actor: "human:operator", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

The PostgreSQL lane (`task check:postgres`) ends within 90 s on the CI runner (4 CPUs), and no test
in it runs over 30 s there, so a loaded runner keeps it inside the 120 s step and the 60 s test
bound.

## Why

On spindle#101 at d81af36f (CI run 46086368337, job PostgreSQL) the lane took 112.3 s of 120 s, and
`adversary_w31_u6::postgres::a_healthy_leader_posts_promptly_on_a_short_and_a_long_lease` took
57.1 s of 60 s. Five more tests ran 33 to 48 s: `adv3_drawn_command_sequences_scan_as_sqlite_does`
48.4 s, `a_reader_counts_a_set_removal_whole` 45.0 s,
`a_move_whose_lock_session_ends_while_it_copies_exits_1_saying_so` 42.2 s,
`adv3_one_group_of_many_writes_lands_as_on_sqlite` 37.8 s and
`reading_a_small_view_does_not_make_the_writer_pay` 33.3 s. The lane needs about 100 s of the
runner's 4 CPUs (measured by wave 31's U6 implementor), so 12 threads leave 10 to 20 s of margin.

## Acceptance

- Three CI runs of the lane each end within 90 s, and its slowest test stays under 30 s.
- The CPU-seconds of one lane run, measured as wave 31 did (tests and database under one 4-CPU
  quota, data in memory), fall by at least a third; the change that cuts them names its case.
