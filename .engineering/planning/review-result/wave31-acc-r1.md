---
format: aep.planning-md/3
id: review-result:wave31-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 31
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

story:event-egress-webhook — the acceptance never says what the POST carries (body format, the event's name, fields or store position), so a stub that only counts requests accepts an empty POST as "posted once", and `events_of_one_component_are_posted_in_commit_order` cannot tell one event from another — .engineering/planning/story/event-egress-webhook.md:83

story:event-egress-webhook — `an_event_committed_while_the_webhook_is_down_is_posted_when_it_returns` gives no time by which the post must arrive, and D7 A's "capped backoff" names no cap, so nothing says this check finishes inside the 60 s test bound — .engineering/planning/story/event-egress-webhook.md:85 (cap at :145)

story:listener-tls — `a_bad_certificate_file_exits_before_listening` checks only "exit 2 before the listening line", and today clap already exits 2 on the unknown `--tls-cert` flag (`Cli::parse()`, crates/spindle-cli/src/main.rs:67), so the test passes before the work; it needs to name the refusal text that names the file — .engineering/planning/story/listener-tls.md:67

story:verified-caller-token — `a_token_run_off_loopback_exits_2` checks only exit 2 before listening, and today clap already exits 2 on the unknown token flag, so the test passes before the work; it needs to name the refusal text, and the token flag itself is never named — .engineering/planning/story/verified-caller-token.md:95

story:spec-change-over-stored-rows — `a_stdin_run_refuses_a_misfit_state_file` checks only exit 3, which `run` already returns for an admission refusal (crates/spindle-cli/src/main.rs:686-688), so a fixture whose changed specification the environment does not admit passes before the work; it needs to name the entity and field on stdout, as acceptance 2 does — .engineering/planning/story/spec-change-over-stored-rows.md:99

story:several-writers-one-postgres — `a_killed_leader_leaves_the_standby_serving` asks for ready `200` "within the lease plus a bound" without naming the bound, so any takeover time passes, while the Outcome promises "within the lease time" — .engineering/planning/story/several-writers-one-postgres.md:99

story:spindle-image-published — "The release's assets hold `IMAGES`, and the digest in it resolves to a manifest list with both architectures" can only be seen in the 0.26.0 release run, which comes after the wave's exit on the PR's CI (wave-31.md § Exit; § Commits: "U2's first push to ghcr.io happens in that release's run"). The body does not say whether the story closes on the stub cases or waits for the release — .engineering/planning/story/spindle-image-published.md:51

**What I read:** all 8 given ids, with `aep plan artifact show` on each, plus the retired `story:event-egress-and-dead-letters` for context. Also `.engineering/waves/wave-31.md`, `aep plan artifact kinds` and `aep plan artifact lifecycle story`. In the tree I checked `Taskfile.yml:178-189`, the exit codes and `Cli::parse` in `crates/spindle-cli/src/main.rs`, the `/__spindle/seed` route, `calls.rs` (`spindle-external/1`, `send`) and the git remote.

**What I could not establish:**
- `aep plan validate` does not exist in this CLI version, so I ran no store validator.
- Out of my lane (lifecycle): U1 `story:postgres-lane-kills-slow-tests`, U7 and U8 are `draft`, not `proposed`. The wave page's exit `active -> implemented` needs `proposed -> active` first.
- Out of my lane (design or parallel safety): U6's acceptance line `a_standby_posts_no_webhook_and_keeps_no_dead_letter` needs U7 and U8, but U6 has no `depends_on` on either; only the merge order covers it.
- Out of my lane (scope): U7's Outcome covers every published event, but its scope covers only the served `run` (`play --component` and `deploy` are listed as not established). U2's "Consumer gap" still says "digest in the release notes", which D1 A replaced.
- Privacy: none. The story bodies name no person, host or home path. `beyond10x` in `ghcr.io/beyond10x/spindle` is the public repository's own owner (`Cargo.toml:11`).
- Wave 30 collisions: the wave page names wave 30's U9 and U12 changes to `cli.rs`, `main.rs`, `serve.rs`, `host.rs`, `surface.rs`, `console.rs`, `router.rs`, `lib.rs`, `capabilities.md` and `README.md`. I found no shared file it leaves unnamed. This is outside my lane in any case.
- Test budget and lanes: every PostgreSQL, container and registry check in the set names its lane. I found no network call beyond loopback, no model call and no new Python.

```findings
- file: .engineering/planning/story/event-egress-webhook.md
  line: 83
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance never says what the POST carries (body format, the event's name, fields or store position), so a stub that only counts requests accepts an empty POST as posted once, and events_of_one_component_are_posted_in_commit_order cannot tell one event from another"
- file: .engineering/planning/story/event-egress-webhook.md
  line: 85
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "an_event_committed_while_the_webhook_is_down_is_posted_when_it_returns gives no time by which the post must arrive, and D7 A's capped backoff names no cap, so nothing says this check finishes inside the 60 s test bound"
- file: .engineering/planning/story/listener-tls.md
  line: 67
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_bad_certificate_file_exits_before_listening checks only exit 2 before the listening line, and today clap already exits 2 on the unknown --tls-cert flag, so the test passes before the work; it needs to name the refusal text that names the file"
- file: .engineering/planning/story/verified-caller-token.md
  line: 95
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_token_run_off_loopback_exits_2 checks only exit 2 before listening, and today clap already exits 2 on the unknown token flag, so the test passes before the work; it needs to name the refusal text, and the token flag itself is never named"
- file: .engineering/planning/story/spec-change-over-stored-rows.md
  line: 99
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_stdin_run_refuses_a_misfit_state_file checks only exit 3, which run already returns for an admission refusal (main.rs:686-688), so a fixture whose changed specification the environment does not admit passes before the work; it needs to name the entity and field on stdout, as acceptance 2 does"
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 99
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_killed_leader_leaves_the_standby_serving asks for ready 200 within the lease plus a bound without naming the bound, so any takeover time passes, while the Outcome promises within the lease time"
- file: .engineering/planning/story/spindle-image-published.md
  line: 51
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "The release's assets hold IMAGES and the digest resolves to a manifest list with both architectures can only be seen in the 0.26.0 release run, which comes after the wave's exit on the PR's CI; the body does not say whether the story closes on the stub cases or waits for the release"
```
