---
format: aep.planning-md/3
id: story:postgres-lane-margin-ci
kind: story
status: draft
title: The PostgreSQL lane keeps a quarter of its bound free on CI
relations:
- serves: vision:spindle
revision: 2
---
## Outcome

The PostgreSQL lane's parallel part on the CI runner ends within 75 s of its 100 s bound, with no
test over 15 s there, so a slower runner or one more lane test does not turn the release gate red.

## Why

Wave 33 (PR 107, run 46151826826) took the CI lane from 60.4 s (PR 106) to 91.6 s of 100 s; the
local lane pinned to 4 CPUs took 70.6 s. The ten slowest on CI were 17.6 s
`adv3_drawn_command_sequences_scan_as_sqlite_does`, 16.4 s
`adv3_one_group_of_many_writes_lands_as_on_sqlite`, 15.5 s
`adversary_state_move::a_long_log_with_groups_across_pages_arrives_whole`, 14.5 s
`adversary_w32_u3::postgres::a_post_under_way_at_sigterm_is_saved_before_the_standby_takes_over`,
14.1 s `adversary_w31_u6::postgres::a_leader_paused_inside_a_write_answers_that_command_503_and_lands_nothing`,
13.5 s `adv3_a_scan_beside_removals_and_recreations_holds_the_state_at_its_version`, 13.4 s
`postgres::replicas::a_leader_paused_inside_a_write_is_taken_over_whole_within_the_lease_and_the_idle_timeout`,
13.2 s `postgres::reading_a_small_view_does_not_make_the_writer_pay`, 12.7 s
`adversary_state_move::a_moved_log_of_0_10_answers_every_scan_and_load_the_file_answers`, 12.5 s
`adversary_w31_u6::postgres::a_write_behind_a_foreign_table_lock_fails_in_time_without_losing_the_lease`.
Wave 33's U3 added about 30 lane cases of 1.4 to 8.9 s each. A step over its budget is a defect;
this one is under it by 8.4 s.

## Acceptance

- The CI run of this change's PR: the PostgreSQL job's `within:` line is at most 75 s of 100 s, and
  its ten slowest tests each take at most 15 s.
- Locally, three runs of `slot --timeout 30m --tokens 4 task check:postgres` (pinned to 4 CPUs)
  each pass and end their parallel part within 60 s.
- `check:postgres` fails, locally and on CI, when any lane test took over 15 s, naming each such
  test with its time, so a slow new test goes red on its own PR (operator's decision D-LM = A,
  2026-11-28).
- Each changed test is shown red against a deliberately broken build (not committed), and its
  before and after times are in its commit body.
- No bound, thread count or timeout is raised, no test is ignored, and no shard is added.
