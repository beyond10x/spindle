---
format: aep.planning-md/3
id: review-result:wave30-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 30
relations:
- reviews: story:view-rows-absent-optional-fields
- reviews: story:view-filter-nested-element-quantifier
- reviews: story:view-filter-map-value-order
- reviews: story:error-payload-generated-fields
- reviews: story:unknown-instance-unsourced-unserved
- reviews: story:create-on-held-identity-unavailable
- reviews: story:decimal-identity-any-spelling
- reviews: story:look-ahead-asks-only-where-it-matters
- reviews: story:related-via-nested-input
- reviews: story:events-cursor-across-restart
- reviews: story:peer-connect-test-steady
- reviews: story:rally-example-reads-stdin
revision: 1
---
needs-revision

story:view-rows-absent-optional-fields — the acceptance names no read path: a row read through a served view (`Host::listed_with`) still leaves the field out under D1 A, while a change to `project` (D1 B) passes both bullets, so the acceptance should name `HostTarget::query_view` and state that a served view row still leaves the field out — .engineering/planning/story/view-rows-absent-optional-fields.md:35 (against D1 A at :94)

story:unknown-instance-unsourced-unserved — "the route (or, with `story:outcome-scoped-refusal`, that outcome) answers 501" accepts a refusal of the whole route, although `story:outcome-scoped-refusal` is `implemented` and D1 A chooses the outcome, so the acceptance should name the outcome and say that the route's other outcomes are still served — .engineering/planning/story/unknown-instance-unsourced-unserved.md:46 (D1 A at :78)

story:error-payload-generated-fields — the `a_seeded_mint_answers_error_values_from_its_seed` bullet joins with "and" a statement that is already true before the work (two runs on one seed already give the same values) to the one statement that shows the change, so the bullet should keep only "the minted identities after the refusal equal a run without it" — .engineering/planning/story/error-payload-generated-fields.md:39

story:decimal-identity-any-spelling — no acceptance covers the `Host::establish` part of D2 A: a wrapper at `reaching` alone passes all four bullets, yet a scenario that establishes `1.0` and then names `1.0` would lose its row, because `establish` writes the key unchanged (`crates/spindle-host/src/host.rs:1270-1279`) while the command key becomes `1` — .engineering/planning/story/decimal-identity-any-spelling.md:95

story:peer-connect-test-steady — item 4, "pass 20 runs in a row under load on the workstation; this run is manual", names no load level and no place where the runs are recorded, so the only check that the tests pass under load rests on someone saying they ran it — .engineering/planning/story/peer-connect-test-steady.md:45

story:peer-connect-test-steady — item 1 is already true before the work, because `listen` takes any loopback address today (the story's own safety fact, :62); it does not show that the two named tests moved to a loopback host of their own, so it should say that each of their cases listens on its own `127.a.b.c` host — .engineering/planning/story/peer-connect-test-steady.md:39

What I read: all 12 of 12 stories (`aep plan artifact show story:<id>`, whole bodies). I also read the wave page, `aep plan artifact kinds`, `aep plan artifact lifecycle story`, and `git blame` on the acceptance lines. In the tree I read `adversary_w26_u2.rs` (`agree` already fails when a view is refused, so U1's first story does change state), the `#[ignore]` lines in `adversary_look_ahead.rs`, `adversary_establish.rs` and `adversary_w27_u5.rs`, `Host::establish`, `world.rs` seed, `rally.md:56-80`, and ESS d7833ee `related_guard.rs:1360,1672`.

What I could not establish:
- Whether `spindle_spec::load`'s error text carries the code `UndeclaredReference` by name, as the `story:related-via-nested-input` acceptance requires. The existing `load.rs` tests only match on file names.
- Whether `an_unsourced_error_field_draws_as_ess_draws` can still be built on an `unknown_instance:` refusal once U3's first story makes such an outcome refuse. If not, its fixture needs a refusal of another kind.
- `story:events-cursor-across-restart`: its Scope calls `a_head_cursor_from_before_a_restart_reads_the_events_after_it` optional, but its Acceptance requires it. This does not set my verdict.
- Out of my lane (design or scope): the Outcomes of `view-rows-absent-optional-fields` and `error-payload-generated-fields` still name ESS 0.86.0, and `unknown-instance-unsourced-unserved` names 0.87.0. The pinned version is 0.99.1.
- Out of my lane (parallel safety): these wave-30 surfaces collide with wave 29. U3 touches `supported.rs`. U6 and U7, and U1 through an inferred line, touch `host.rs`.
- No person, organisation, host or home path appears in any of the 12 bodies.

```findings
- file: .engineering/planning/story/view-rows-absent-optional-fields.md
  line: 35
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance names no read path: a row read through a served view (`Host::listed_with`) still leaves the field out under D1 A, while a change to `project` (D1 B) passes both bullets, so the acceptance should name `HostTarget::query_view` and state that a served view row still leaves the field out"
- file: .engineering/planning/story/unknown-instance-unsourced-unserved.md
  line: 46
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: undecided
  message: "\"the route (or, with `story:outcome-scoped-refusal`, that outcome) answers 501\" accepts a refusal of the whole route, although `story:outcome-scoped-refusal` is `implemented` and D1 A chooses the outcome, so the acceptance should name the outcome and say that the route's other outcomes are still served"
- file: .engineering/planning/story/error-payload-generated-fields.md
  line: 39
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the `a_seeded_mint_answers_error_values_from_its_seed` bullet joins with \"and\" a statement that is already true before the work (two runs on one seed already give the same values) to the one statement that shows the change, so the bullet should keep only \"the minted identities after the refusal equal a run without it\""
- file: .engineering/planning/story/decimal-identity-any-spelling.md
  line: 95
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "no acceptance covers the `Host::establish` part of D2 A: a wrapper at `reaching` alone passes all four bullets, yet a scenario that establishes `1.0` and then names `1.0` would lose its row, because `establish` writes the key unchanged (`crates/spindle-host/src/host.rs:1270-1279`) while the command key becomes `1`"
- file: .engineering/planning/story/peer-connect-test-steady.md
  line: 45
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "item 4, \"pass 20 runs in a row under load on the workstation; this run is manual\", names no load level and no place where the runs are recorded, so the only check that the tests pass under load rests on someone saying they ran it"
- file: .engineering/planning/story/peer-connect-test-steady.md
  line: 39
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "item 1 is already true before the work, because `listen` takes any loopback address today (the story's own safety fact, :62); it does not show that the two named tests moved to a loopback host of their own, so it should say that each of their cases listens on its own `127.a.b.c` host"
```
