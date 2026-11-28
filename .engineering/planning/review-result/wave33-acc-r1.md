---
format: aep.planning-md/3
id: review-result:wave33-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 33
relations:
- reviews: story:listener-limits
- reviews: story:new-leader-write-while-old-paused
- reviews: story:seed-record-after-a-component-is-removed
- reviews: story:entity-invariant-over-stored-rows
revision: 1
---
needs-revision

story:listener-limits — the `run_flags_bound_the_listener` check does not require the refused head to be under the 16 KiB default, and nothing in it shows `--max-connections` or `--request-timeout` taking effect, so a `run` that parses the three flags but never passes them to `Surface::limited` still passes — .engineering/planning/story/listener-limits.md:108

story:listener-limits — the cap test, `a_connection_over_the_cap_waits_until_one_closes`, says "a third gets no answer while both stay open" without saying how long the test waits for that answer, so the check gives a different result depending on how long it waits — .engineering/planning/story/listener-limits.md:79

story:seed-record-after-a-component-is-removed — no acceptance line checks D2 A ("When no current component's lease table holds a record, the start looks once…"), for example that a start whose current table holds a record ignores a different record in a lease table no current component owns, so this check reads the same under D2 A and D2 B — .engineering/planning/story/seed-record-after-a-component-is-removed.md:41

What I read: all 4 stories, in full, with `aep plan artifact show story:<id>` (listener-limits, new-leader-write-while-old-paused, seed-record-after-a-component-is-removed, entity-invariant-over-stored-rows), plus the wave page and the story postgres-lane-margin. Against the tree I checked:
- the PostgreSQL lane's filter and `.config/nextest.toml`
- the code of `adversary_w31_u6.rs`, `adversary_w32_u4.rs` and `spec_change.rs`, plus `fit.rs` `Shapes::misfits`, the `observe.rs` classes, the `LEASE_LOST` refusal and the fixtures
- eventlog 0.10.0, the version in `Cargo.lock`: its publication lock key and `PoolOptions` (lock timeout 2 s)

What I could not establish:
- Outside my lane (scope, claim accuracy): story:new-leader-write-while-old-paused, Scope line 139, says `a_resumed_leader_posts_no_event_the_new_leader_committed` already retries a `503` until it gets `201`. In the tree it has `assert_eq!(status, 201)` with no retry (`crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs:400`). The Acceptance line is still clear: the retry has to be added.
- Outside my lane: the Scope test list of story:listener-limits (line 149) still has the old name `connections_over_the_cap_are_refused` and is missing four of the tests in Acceptance. Decided when scoped overrides it.
- Test budget: no test that adds more than about 10 s to the PostgreSQL lane goes unstated. U2's 15 s reproduction is stated, and the rewritten case is bounded at 30 s. U3's six tests are "expected under 10 s", but none has been timed. Every new lane test is under the lane's `postgres::` filter.
- Public repository: none of the four story bodies names a person, organisation, host or home path. No Python, and no network access beyond loopback.

```findings
[{"file": ".engineering/planning/story/listener-limits.md", "line": 108, "category": "acceptance", "severity": "warning", "verdict": "needs-revision", "origin": "introduced", "message": "the run_flags_bound_the_listener check does not require the refused head to be under the 16 KiB default, and nothing in it shows --max-connections or --request-timeout taking effect, so a run that parses the three flags but never passes them to Surface::limited still passes"},
 {"file": ".engineering/planning/story/listener-limits.md", "line": 79, "category": "acceptance", "severity": "warning", "verdict": "needs-revision", "origin": "introduced", "message": "the cap test, a_connection_over_the_cap_waits_until_one_closes, says 'a third gets no answer while both stay open' without saying how long the test waits for that answer, so the check gives a different result depending on how long it waits"},
 {"file": ".engineering/planning/story/seed-record-after-a-component-is-removed.md", "line": 41, "category": "acceptance", "severity": "warning", "verdict": "needs-revision", "origin": "introduced", "message": "no acceptance line checks D2 A, for example that a start whose current table holds a record ignores a different record in a lease table no current component owns, so this check reads the same under D2 A and D2 B"}]
```
