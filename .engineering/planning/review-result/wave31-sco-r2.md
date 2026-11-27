---
format: aep.planning-md/3
id: review-result:wave31-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 31
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

All eight stories hold; I found nothing new that blocks the wave.

**Round-1 finding**
- story:verified-caller-token — Outcome and D3 (the off-loopback decision) — **fixed**. D3 is now B on the coordinator's default: off loopback a token run is served only over the listener's TLS, and without `--tls-cert` and `--tls-key` it exits 2. The Outcome states that limit. The acceptance line `a_token_run_off_loopback_exits_2` also checks the TLS case, where the same run answers a valid token over TLS. `depends_on story:listener-tls` is added. The deferred part no longer lacks an owner, because the story takes the whole off-loopback case itself.

**New findings from the revision**
- None. I read the full diff of 278f3126 for story-level reach beyond the option picked.
  - The `spindle-egress/1` body in story:event-egress-webhook states the payload "each committed event" already promised. It adds no capability.
  - The 5 s backoff cap in D7 and the `status.rs`, `status.json` and `status.md` rows are consequences of promises already made, not new ones.
  - The wave-30 overlap list on the wave page now names `host.rs`, `lib.rs`, the test `main.rs` lists, `README.md`, `console.rs` and `router.rs` as line moves only. That closes my round-1 note outside my lane.
- Public-repo check: no person, home path or private host appears in the eight bodies or the wave page.

**Checked and not findings**
- story:spindle-image-published now closes on the stub cases in the PR's CI, and the `IMAGES` asset is verified in the 0.26.0 release run. The story and the wave page both say so, so this is a named deferral and not a silent narrowing. Its Consumer gap now says `IMAGES` asset, matching D1 A.
- story:spec-change-over-stored-rows no longer says "or migrated" in its title. This matches option A.
- Coverage count: I re-checked the 29 promises from round 1. All 29 trace to an item; the one narrowed in round 1 is now fixed.

**Out of my lane**
- In story:verified-caller-token, the acceptance bullet list is split by the paragraph "No exit-2 line above passes on clap's own exit 2…". The last bullet, covering the three unit tests in `token.rs`, falls after it. That is formatting, and acceptance-critic territory. It does not affect scope.
