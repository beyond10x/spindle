---
format: aep.planning-md/3
id: review-result:wave35-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 35
relations:
- reviews: story:link-bound-setting
- reviews: story:observe-label-vocabulary
- reviews: story:unknown-commit-in-seed-and-retries
revision: 1
---
Output was truncated; I'll read each story separately.
needs-revision

- story:observe-label-vocabulary — the Acceptance names no check that the label vocabulary (`domain`, `component`, `kind`, `name`) or the cardinality rule exists. `task check:the_observe_domain_validates` passes on today's `observe.yaml`, so that bullet reads the same before and after. Name the type or text the model must hold, or a test that compares it to `SPECIFIED_LABELS` — `.engineering/planning/story/observe-label-vocabulary.md:71`
- story:observe-label-vocabulary — "none" is not defined as a `unit` value. The Outcome lists it unquoted after the values (`:40`) and D3 says "has unit none" (`:107`), but the test requires every family to carry "a `unit` from the specified values". It does not say whether the fixture and `FAMILIES` hold a `"none"` variant or leave the field absent, so the compared JSON is undefined for the families that count things — `.engineering/planning/story/observe-label-vocabulary.md:80`
- story:unknown-commit-in-seed-and-retries — the docs bullet is "`task docs-check` stays green with `components-and-bindings.md` … updated". It names no sentence the docs must now say, and `docs-check` is green before the edit too. State the new text, for example that an unknown outcome is kept as a dead letter and not sent again. Also state that the seed-table sentence about `--state` and leased runs is amended — `.engineering/planning/story/unknown-commit-in-seed-and-retries.md:120`

I found nothing else against the wave's constraints. No test is over 60 s, and the test step stays under 120 s. No test makes a network request outside loopback or a model call, and none is added to the PostgreSQL lane. No Python is added. I grepped the three bodies and the wave page for `/home`, a person's name, a URL and an address. None appears. The "slot" and "operator" references are tools and roles, not persons.

What I read: 3 of 3 ids (`story:link-bound-setting`, `story:observe-label-vocabulary`, `story:unknown-commit-in-seed-and-retries`) with `aep plan artifact show <id>`, plus `.engineering/waves/wave-35.md`, `aep plan artifact kinds` and `aep plan artifact lifecycle story`. I checked these in the tree at head 19e39cf1:
- the `Taskfile.yml:347` fixture-validate line;
- the test names at `metrics.rs:662` and `observe.rs:993`;
- the never-listens test and its 45 s ceiling at `adversary_processes.rs:737`;
- the `MetricFamily` type at `observe.yaml:523`;
- `Recorded` and `Became` in `seed.rs`.

What I could not establish:
- Whether the U3 seed unit test can be built, because the Scope itself lists as "still open" whether `Host::supplied`/`with_stores` admits a seed fixture. That is design and scope, out of my lane.
- U3 bundles two outcomes, a bound-command retry and the seed. I did not raise this, because the acceptance is a list of named tests that all must pass. Whether the story should be split is for the scope critic.
- U1's "ends within 3 s" is enforced only by the 10 s wait ceiling. I did not raise this, because the ceiling still fails a regression to 30 s.
- I did not run the tasks.

```findings
- file: .engineering/planning/story/observe-label-vocabulary.md
  line: 71
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the acceptance names no check that the label vocabulary (domain, component, kind, name) or the cardinality rule exists: `task check:the_observe_domain_validates` passes on today's observe.yaml, so it reads the same before and after"
- file: .engineering/planning/story/observe-label-vocabulary.md
  line: 80
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "'none' is not defined as a unit value (unquoted at :40, 'has unit none' at :107) while the test requires every family to carry a unit from the specified values, so whether the fixture and FAMILIES hold a 'none' variant or an absent field is undefined"
- file: .engineering/planning/story/unknown-commit-in-seed-and-retries.md
  line: 120
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the docs bullet 'task docs-check stays green with components-and-bindings.md ... updated' names no sentence the docs must now say, and docs-check is green before the edit too, so it does not show the docs changed"
```
