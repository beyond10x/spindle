---
format: aep.planning-md/3
id: review-result:wave31-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 31
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

story:verified-caller-token — The story promises that "no request proves a caller by a header any proxy could copy" (verified-caller-token.md:70), and the problem it fixes is a consumer's proxy sitting off loopback (`:77`). But D3 A makes a token run off loopback exit 2 and leaves lifting that to "a later story" (`:119-120`) that no artifact in the store claims. The token therefore cannot replace the header in the deployment the story was written for, and the deferred part has no owner. The body should name or draft that follow-up, or state the loopback-only limit in its Outcome. — .engineering/planning/story/verified-caller-token.md:119

**What I read:** 8 stories in the set, plus the rejected parent `story:event-egress-and-dead-letters`, `approval-record:architecture-picks-2026-11-27`, `epic:production-service`, `story:request-id-idempotency`, `story:console-routes-off`, and `wave-31.md` and `wave-30.md`. Commands: `aep plan artifact show` for each, `aep plan artifact graph`, `git diff 8eaea47a..HEAD` on the story files (to read the pre-scoping Outcome and Acceptance as the promise list), and grep. I listed 29 promises: from the six option-A picks, the parent egress/dead-letter story's Outcome and Acceptance, and the `AGENTS.md` test-budget rule behind U1. 28 are fully claimed by an item. 1 is narrowed (the finding above).

**Checked and not findings:**
- `a_retried_request_lands_once` is still covered. It moved to `story:request-id-idempotency`, which is outside the set and now holds the line.
- The original several-writers "a request reaches any of them" and `a_row_set_guard_holds_across_processes` lines were option B's and are correctly gone.
- Migration is dropped under option A. The spec-change title still says "or migrated", which is wording only.
- No two items claim the same outcome. U5, U7 and U8 each claim their own reserved stream and its "no published event" check.
- No item reaches beyond its parent. U1 comes from the `AGENTS.md` rule that the first budget story makes the runner kill a test at 60 s. U8 also counts periodic commands (tick) as dead letters, a small extension recorded in its D6 A.
- No person, home path or private host appears in the bodies. `ghcr.io/beyond10x/spindle` is the repository's own public org, already in `README.md`, and "host operator" is a role.

**What I could not establish:**
- U7 defers to its "Not established when scoped" list whether `deploy` and `play --component` post webhooks. The parent promises "each committed event", so this is a named, honest omission.
- Outside my lane (parallel-safety), flagged because the brief asked: wave-31.md:108 names only wave 30's U9 and U12 as changing wave-31 surfaces. Wave 30 also changes:
  - `spindle-host` `host.rs` in U1, U6 and U7 (wave 31 U5, U6, U8)
  - `spindle-kernel` `lib.rs` in U5 and U6 (wave 31 U5, U7, U8)
  - the test module lists of `spindle-kernel` in U3, U6, U11 (wave 31 U5, U7, U8), `spindle-ingress` in U3 (wave 31 U3) and `spindle-host` in U1, U8 (wave 31 U8)
  - `README.md` in U3 (wave 31 U2, U7, U8)

  Wave 30 merges before wave 31 forks, so these only shift the cited line numbers.

```findings
- file: .engineering/planning/story/verified-caller-token.md
  line: 119
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the story promises that \"no request proves a caller by a header any proxy could copy\" (verified-caller-token.md:70), and the problem it fixes is a consumer's proxy sitting off loopback (:77), but D3 A makes a token run off loopback exit 2 and leaves lifting that to \"a later story\" (:119-120) that no artifact in the store claims, so the token cannot replace the header in the deployment the story was written for and the deferred part has no owner; the body should name or draft that follow-up, or state the loopback-only limit in its Outcome"
```
