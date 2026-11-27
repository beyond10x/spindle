---
format: aep.planning-md/3
id: review-result:wave28-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 28
relations:
- reviews: story:console-routes-off
- reviews: story:structured-json-logs
- reviews: story:metrics-series-bounded
- reviews: story:seed-readiness
- reviews: story:health-and-graceful-drain
- reviews: story:entity-event-history
- reviews: story:events-record-their-caller
- reviews: story:environment-binds-external-decisions
- reviews: story:replay-with-decisions
- reviews: story:one-time-response-admission
revision: 1
---
approve

I read the whole of each of the 10 stories and found nothing that would change what was drafted. Each of the 7 round-1 findings in my lane (`review-result:wave28-acc-r1`) is resolved.

Round-1 findings, one by one:
- **`story:one-time-response-admission`**: Acceptance bullet 2 now says it fails on 0.22.0, and bullet 3 is split off as guards that pass on 0.22.0 and stay green. The Outcome now describes the `run --scenario` error (not the stale `target.rs` refusal).
- **`story:seed-readiness`**: the new bullet `a_view_holds_the_last_entry_once_ready_reads_200` reads a view row right after the first 200.
- **`story:metrics-series-bounded`**: the bound is now observable. Served runs over 30 and 300 commands write the same series count in aggregated mode and a different count in the default mode.
- **`story:entity-event-history`**: the new bullet `a_store_written_before_reads_history_only_from_new_writes` covers a SQLite state written by 0.22.0. It answers only events of writes made after the upgrade, and the changelog says so.
- **`story:structured-json-logs`**: D2 now has `json_logs_on_a_stdin_run_exit_1`, and `request_id` is observed through `a_seed_entry_line_names_its_request_id`. `seed.rs:663` already prints a refused-entry line with the request id, so the bullet can be run.
- **`story:events-record-their-caller`**: the new bullet `a_split_run_binding_event_records_an_unknown_cause` covers D2.
- **`story:environment-binds-external-decisions`**: the new bullet `a_decisions_file_binding_without_a_file_exits_1` covers D3.

Fresh pass:
- **`story:console-routes-off`**, **`story:health-and-graceful-drain`** and **`story:replay-with-decisions`** each have named, observable bullets. The state change in each is stated: a route going to 404, readiness going 200 to 503, and `--decisions` no longer refused.
- Every decision under "Decided when scoped" has a bullet, except `story:structured-json-logs` D3 (no schema marker) and D4 (no per-request line), which are negative and need none.

What I read: 10 stories in full with `aep plan artifact show story:<id>` for each, plus `aep plan artifact show review-result:wave28-acc-r1`. I read `references/plan-critic-acceptance.md` and `references/critic-rubric.md`, and ran `git grep` on `crates/spindle-cli/src/seed.rs` (head 370e9da6).

What I could not establish:
- Whether the PostgreSQL and IndexedDB contract cases (in `story:events-record-their-caller` and `story:entity-event-history`) run in the gate. This is unchanged from round 1.
- Whether the new timings and series counts hold, since no test exists yet.
- The public-repository check for people, organisations, hosts and home paths is outside my lane. I saw none in the acceptance and decision sections I read.
- Out of my lane, not counted in the verdict: `story:health-and-graceful-drain` and `story:seed-readiness` both touch the `/__spindle/` routes, and `story:one-time-response-admission` and `story:replay-with-decisions` share `replay.rs`.

```findings
[]
```
