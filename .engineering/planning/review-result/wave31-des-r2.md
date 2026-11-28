---
format: aep.planning-md/3
id: review-result:wave31-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 31
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
approve

All five round-1 findings are fixed, and the revision introduced nothing new in my lane.

Round-1 findings:
- **story:several-writers-one-postgres, depends_on U7 and U8:** fixed. The graph now has `several-writers-one-postgres -> event-egress-webhook` and `-> dead-letters-kept-and-replayed`, both `depends_on`. The acceptance line `a_standby_posts_no_webhook_and_keeps_no_dead_letter` says both are `depends_on` of the story.
- **story:several-writers-one-postgres, depends_on U1:** fixed. The edge `-> postgres-lane-kills-slow-tests` is in the graph.
- **story:spec-change-over-stored-rows, depends_on U1:** fixed. The edge is in the graph and recorded in the story's Scope.
- **story:event-egress-webhook, depends_on U1:** fixed. The edge is in the graph and recorded in the story's Scope.
- **story:dead-letters-kept-and-replayed, depends_on U1:** fixed. The edge is in the graph and recorded in the story's Scope.

The wave page's "`depends_on` edges inside the wave" paragraph matches the graph exactly.

New edge from the revision: U4 `depends_on` U3 (D3 B). The `a_token_run_off_loopback_exits_2` acceptance reads `--tls-cert`, `--tls-key` and TLS serving, which U3 produces, so the edge is correct. Merge order U3 before U4 agrees with it. The other U4 acceptance lines read nothing a sibling produces.

**What I read:**
- All 8 stories and wave-31.md in full, plus `review-result:wave31-des-r1`, `git show 278f3126` and the revised story bodies.
- Commands: `aep plan artifact show`, `relations`, `graph`, `validate` and `waves`.
- `validate` reports `valid` plus old review-outcome notes on earlier waves only, which are not my findings.
- **Edges:** I walked the `depends_on` edges of all 8 stories, including the one leaving the set (U2 to `story:ingress-bind-address`, already implemented). The `informed_by` edges are "shaped by", not "needs first". There is no cycle.
- **Shape of the set:** the chain U1 to {U5, U7, U8} to U6, with U3 to U4 and U2 alone, does not serialise the set. U7 and U8 stay independent of each other.
- **Acceptance reads:** I traced 11 reads to a producer in the set (U4 to U3; U5, U6, U7 and U8 to U1; U6 to U5, U7 and U8). An edge records all 11.
- **Public repository check:** no home path or personal name in the 8 story bodies or the wave page.

**What I could not establish:**
- **Out of my lane (parallel safety):** U5, U7 and U8 each add a reserved `spindle:log` stream and its skip in one `sqlite.rs:1351` hunk. U4 and U6 both edit `listener.rs` `answer`. U7 before U4 orders the `calls.rs` helpers. All of these are ordered only by the wave page's merge order, which is the file-sharing case, not half an abstraction in each item.
- **Out of my lane (acceptance):** in `verified-caller-token.md` the paragraph "No exit-2 line above passes on clap's own exit 2" sits in the middle of the Acceptance bullet list, between `the_console_routes_are_off_on_a_token_run` and the unit-test bullet. Cosmetic.

```findings
[]
```
