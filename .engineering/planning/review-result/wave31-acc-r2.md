---
format: aep.planning-md/3
id: review-result:wave31-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 31
relations:
- reviews: story:postgres-lane-kills-slow-tests
- reviews: story:spindle-image-published
- reviews: story:listener-tls
- reviews: story:verified-caller-token
- reviews: story:spec-change-over-stored-rows
- reviews: story:several-writers-one-postgres
- reviews: story:event-egress-webhook
- reviews: story:dead-letters-kept-and-replayed
revision: 1
---
needs-revision

story:several-writers-one-postgres — `a_killed_leader_leaves_the_standby_serving` times ready `200` within 2.67 s of the kill, but D3 A makes a standby open the stores, build the host and seed after it takes the lease (and before it answers ready), and no standby poll interval is named. The bound can fail even when the takeover itself is on time. The acceptance needs to time the lease take, or name the poll interval and add a bound for opening and seeding. — .engineering/planning/story/several-writers-one-postgres.md:107 (D3 A at :136, D7 A at :143)

**Round-1 findings** (`review-result:wave31-acc-r1`)
1. `story:event-egress-webhook` POST contents (blocker): fixed. The Outcome and Acceptance now state the `spindle-egress/1` body (`format`, `event`, `component`, `position`, `fields`). Every test line reads bodies, and the order test compares `position` and event names.
2. `story:event-egress-webhook` down-webhook time bound (warning): fixed. The backoff is capped at 5 s in D7 A, and the test is "5 s later ... ends within 20 s".
3. `story:listener-tls` bad certificate (warning): fixed. The test now reads an error line that names the flag and the file, or both files for a mismatched pair, so clap's exit 2 for an unknown flag no longer passes it.
4. `story:verified-caller-token` token off loopback (warning): fixed. `--caller-token <FILE>` is named, the exit-2 lines read their error lines, and an explicit sentence rules out clap's exit 2.
5. `story:spec-change-over-stored-rows` stdin refusal (warning): fixed. The test names entity and field on stdout and says "an exit 3 from admission alone does not pass it".
6. `story:several-writers-one-postgres` takeover bound (warning): fixed in form, since the bound is now named (lease plus one renew interval, 2.67 s with the 2 s lease). The new finding above concerns what it measures.
7. `story:spindle-image-published` closing point (warning): fixed. The story closes on the stub cases of the pull request's CI run. `IMAGES` is verified in the 0.26.0 release run, and wave-31.md § Exit says the same.

**New findings:** only the one above. The revision introduced it, by naming the bound. Nothing else it changed adds a defect.

**What I read**
- All 8 ids, read from `.engineering/planning/story/*.md` in the worktree (`awk` over the bodies, plus `aep plan artifact show` for status and relations).
- The round-1 review, via `aep plan artifact show review-result:wave31-acc-r1`.
- `git show 278f3126` (stat and the wave-31.md diff), and a grep of that diff for home paths and personal names (none found).
- All 8 stories are `proposed`.

**What I could not establish**
- `story:event-egress-webhook`, `a_posted_event_is_not_posted_again_after_a_restart`: the story does not say how the test knows the cursor is saved before the restart. I could not tell whether this needs a fix, so I did not raise it.
- Out of my lane (scope): `story:event-egress-webhook`'s Outcome covers every published event, but its scope covers only the served `run`, and `play --component` and `deploy` stay open under "Not established".
- Out of my lane (parallel safety): I found no wave-31 surface that collides with wave 30 without the wave page saying so. Round 1 raised no such finding.
- Privacy: nothing in the story bodies names a person, host or home path. `ghcr.io/beyond10x/spindle` is the repository's own owner.
- Test budget: every test that needs PostgreSQL or a registry names its lane, and I found no outside network, model call or new Python.

```findings
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 107
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_killed_leader_leaves_the_standby_serving times ready 200 within 2.67 s of the kill, but D3 A makes a standby open the stores, build the host and seed after it takes the lease and before it answers ready, and no standby poll interval is named, so the bound can fail with an on-time takeover; the acceptance must time the lease take, or name the poll interval and add a bound for opening and seeding"
```
