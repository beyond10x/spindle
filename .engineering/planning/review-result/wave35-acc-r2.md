---
format: aep.planning-md/3
id: review-result:wave35-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 35
relations:
- reviews: story:link-bound-setting
- reviews: story:observe-label-vocabulary
- reviews: story:unknown-commit-in-seed-and-retries
revision: 1
---
needs-revision

- story:link-bound-setting — the second Acceptance bullet says `Cli::try_parse_from` "refuses `--link-within 0`" but does not say the line carries the `play` arguments that are required (`spec`, `--environment`), so a call without them also refuses, and the "at least 1" bound can pass unobserved. State that the line has `spec` and `--environment` and the same line with `--link-within 1` parses. — `.engineering/planning/story/link-bound-setting.md:52` (`play` requires `spec` and `--environment`: `crates/spindle-cli/src/cli.rs:730-735`)

All three round-1 findings are resolved in b9f8e424:

- **`story:observe-label-vocabulary`, no check that the vocabulary exists.**
  - The new test `the_label_vocabulary_is_the_specified_vocabulary` reads `spindle.observe.MetricLabel.schema.json` and asserts the `enum` `["domain","component","kind","name"]`, the `kind` values and the rule's sentence in the description.
  - It fails on today's tree: I listed `ess/generated/jsonschema/schema/types` and no `MetricLabel` file exists.
  - The description is the `naming.summary`, as in `spindle.observe.MetricKind.schema.json`.
  - The `observe.yaml` citations hold.
- **`story:observe-label-vocabulary`, `"none"` undefined.** D5 A and the Outcome make `"none"` an explicit quoted value of `unit`. The fixture and `FAMILIES` both write it (`observe-label-vocabulary.md:91`).
- **`story:unknown-commit-in-seed-and-retries`, docs bullet.** The bullet now names the sentence each page must say, and the `--seed` help and `cli.md:153` change is stated with its replacement text. I checked each cited paragraph in the tree:
  - `components-and-bindings.md:52-57`;
  - `environment-and-admission.md:260`;
  - `environment-and-admission.md:293-296`;
  - `environment-and-admission.md:372-375`;
  - `cli.rs:547-548` and `cli.rs:559-561`.

The revision broke nothing in my lane. The wave constraints still hold:

- Every test is under 1 s and none is added to the PostgreSQL lane.
- No new Python, no network request outside loopback, no model call.
- A grep of the bodies found no person, organisation, host, URL or home path.

What I read: 3 of 3 ids (`story:link-bound-setting`, `story:observe-label-vocabulary`, `story:unknown-commit-in-seed-and-retries`) with `aep plan artifact show <id>`, plus `review-result:wave35-acc-r1`, the `git diff --stat` of b9f8e424 and the tree lines cited above. I ran no task.

What I could not establish:
- Whether `Host::supplied`/`with_stores` admits a seed fixture. The Scope still lists it as open; that is design and scope, out of my lane.
- Out of my lane: the Scope's "Test time" line in `observe-label-vocabulary.md:168` says "no new test", while the Acceptance now adds one. That is the scope critic's.

```findings
- file: .engineering/planning/story/link-bound-setting.md
  line: 52
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the acceptance says Cli::try_parse_from refuses `--link-within 0` but not that the line carries the required `spec` and `--environment` of `play`, so a line without them also refuses and the at-least-1 bound can pass unobserved; state that the same line with `--link-within 1` parses"
```
