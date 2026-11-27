---
format: aep.planning-md/3
id: review-result:wave31-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 31
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

story:several-writers-one-postgres — its acceptance line `a_standby_posts_no_webhook_and_keeps_no_dead_letter` reads the egress binding that story:event-egress-webhook adds and the dead letters that story:dead-letters-kept-and-replayed adds, and only prose ("this unit merges after both") records that order, so it needs `depends_on story:event-egress-webhook` and `depends_on story:dead-letters-kept-and-replayed` beside its `depends_on story:spec-change-over-stored-rows` — .engineering/planning/story/several-writers-one-postgres.md:106-108, .engineering/waves/wave-31.md:131, `aep plan artifact graph`

story:spec-change-over-stored-rows — its PostgreSQL acceptance lines are checked in the `task check:postgres` lane, and the 60 s kill in that lane comes from story:postgres-lane-kills-slow-tests; only the wave page's merge order records this ("U1 first"), so it needs `depends_on story:postgres-lane-kills-slow-tests` — .engineering/planning/story/spec-change-over-stored-rows.md:103, .engineering/waves/wave-31.md:115

story:several-writers-one-postgres — its PostgreSQL-lane acceptance lines are checked in the lane whose 60 s kill story:postgres-lane-kills-slow-tests adds, and no edge records that, so it needs `depends_on story:postgres-lane-kills-slow-tests` — .engineering/planning/story/several-writers-one-postgres.md:93, .engineering/planning/story/postgres-lane-kills-slow-tests.md:38

story:event-egress-webhook — its acceptance line `postgres::an_egress_cursor_survives_on_postgres` is checked in the lane whose 60 s kill story:postgres-lane-kills-slow-tests adds, and no edge records that, so it needs `depends_on story:postgres-lane-kills-slow-tests` — .engineering/planning/story/event-egress-webhook.md:94, .engineering/waves/wave-31.md:115

story:dead-letters-kept-and-replayed — its acceptance line `postgres::a_dead_letter_survives_on_postgres` is checked in the lane whose 60 s kill story:postgres-lane-kills-slow-tests adds, and no edge records that, so it needs `depends_on story:postgres-lane-kills-slow-tests` — .engineering/planning/story/dead-letters-kept-and-replayed.md:106, .engineering/waves/wave-31.md:115

**What I read:** all 8 stories, the retired story:event-egress-and-dead-letters and story:ingress-bind-address (whole bodies, via `aep plan artifact show`), plus `wave-31.md`, the Surfaces section of `wave-30.md` and `sqlite.rs` around lines 90-110 and 1340-1356. Commands: `aep plan artifact relations`, `graph` (dot format), `validate` and `waves`.
- **Edges:** I walked 41 edges that touch the set, including ones that leave it: `depends_on` U2→story:ingress-bind-address (already implemented), story:console-image-published and story:kubernetes-render `depends_on` U2, and U6 `informed_by` story:request-id-idempotency. The `depends_on` edges have no cycle, and they do not chain the set into one queue.
- **Acceptance reads:** 6 reads go to a producer inside the set (U6→U7, U6→U8, and U5/U6/U7/U8→U1). No edge records any of them.
- **No split abstraction:** U7 and U8 can each be shown working alone.
- **Shared skip in `sqlite.rs`:** U5, U7 and U8 each add their own reserved `spindle:log` stream, plus a skip at `sqlite.rs:1351` in one shared hunk. That is a shared file, not half an abstraction in each.

**What I could not establish:**
- `validate` reports only problems in older review records (4 reviews whose findings are prose only, 23 with no recorded outcome). Nothing about this set, so none of it is my finding.
- **Out of my lane (scope):** U4 D3 A exits 2 when a token run is off loopback, and U3 D5 A keeps the header flags required off loopback. So in this set a verified-token run works only on loopback, and the off-loopback case needs a story that is not planned.
- **Out of my lane (acceptance):** U6's `depends_on` U5 is justified as guarding the rolling deploy, but no U6 acceptance line tests a standby on a newer specification taking the lease.
- **Out of my lane (scope):** U7 and U8 leave open whether `forget_events` and `state move` carry the new `spindle:log` streams.
- **Out of my lane (parallel safety):** the wave-31 page names only wave 30's U9 and U12 as overlaps. Wave 30's U1, U6 and U7 also change `spindle-host` `host.rs`, wave 30's U5 and U6 change `spindle-kernel` `lib.rs`, and several wave-30 units change the test `main.rs` lists. Wave 31 forks after wave 30 merges, so this only shifts line numbers.
- **Public repository check:** no person, host or home path in any story body. `ghcr.io/beyond10x/spindle` in U2 is the repository's own public GitHub organisation, the same as `origin`.

```findings
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 106
  category: design
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "its acceptance line a_standby_posts_no_webhook_and_keeps_no_dead_letter reads the egress binding that story:event-egress-webhook adds and the dead letters that story:dead-letters-kept-and-replayed adds, and only prose (\"this unit merges after both\") records that order, so it needs depends_on story:event-egress-webhook and depends_on story:dead-letters-kept-and-replayed beside its depends_on story:spec-change-over-stored-rows"
- file: .engineering/planning/story/spec-change-over-stored-rows.md
  line: 103
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its PostgreSQL acceptance lines are checked in the task check:postgres lane, and the 60 s kill in that lane comes from story:postgres-lane-kills-slow-tests; only the wave page's merge order records this (\"U1 first\"), so it needs depends_on story:postgres-lane-kills-slow-tests"
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 93
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its PostgreSQL-lane acceptance lines are checked in the lane whose 60 s kill story:postgres-lane-kills-slow-tests adds, and no edge records that, so it needs depends_on story:postgres-lane-kills-slow-tests"
- file: .engineering/planning/story/event-egress-webhook.md
  line: 94
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance line postgres::an_egress_cursor_survives_on_postgres is checked in the lane whose 60 s kill story:postgres-lane-kills-slow-tests adds, and no edge records that, so it needs depends_on story:postgres-lane-kills-slow-tests"
- file: .engineering/planning/story/dead-letters-kept-and-replayed.md
  line: 106
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance line postgres::a_dead_letter_survives_on_postgres is checked in the lane whose 60 s kill story:postgres-lane-kills-slow-tests adds, and no edge records that, so it needs depends_on story:postgres-lane-kills-slow-tests"
```
