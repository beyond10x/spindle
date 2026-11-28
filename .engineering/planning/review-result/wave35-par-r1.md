---
format: aep.planning-md/3
id: review-result:wave35-par-r1
kind: review-result
status: active
title: Plan critic (parallel-safety), round 1, on wave 35
relations:
- reviews: story:link-bound-setting
- reviews: story:observe-label-vocabulary
- reviews: story:unknown-commit-in-seed-and-retries
revision: 1
---
needs-revision

- story:unknown-commit-in-seed-and-retries — its acceptance (`the_generated_types_have_no_drift` "regenerated on top of `story:observe-label-vocabulary`'s merged change") reads the generated files that story:observe-label-vocabulary writes. No `depends_on` edge runs from this story to it, and the wave page says "No `depends_on` edge inside the wave". The merge order lives only in prose, so `aep plan artifact waves` would schedule both at once. Add a `depends_on` edge from this story to `story:observe-label-vocabulary`, or merge the two items. — .engineering/planning/story/unknown-commit-in-seed-and-retries.md:118, .engineering/waves/wave-35.md:59
- story:unknown-commit-in-seed-and-retries — the U3 doc scope omits `crates/spindle-cli/src/cli.rs`. The `--seed` help (`cli.rs:546-562`) says a rejected entry "did not run" and that a restart "sends the rejected entries again". Both statements become wrong under D5 A and D6 A: the entry is done, not sent again, and counted under `rejected`. The help is generated into `website/docs/reference/cli.md:153`. If U3 edits it, U3 collides with U1 on `cli.rs` and the generated `cli.md`, and the wave page's "U1 shares only `CHANGELOG.md`" is false. If U3 leaves it, the generated reference stays stale. Either state in the body that U3 leaves `cli.rs` and `cli.md` untouched, or record the shared file with an ordering edge, or split the surface. This rests on an inferred surface: the body does not name `cli.rs`, but the help text states the rule U3 changes. — crates/spindle-cli/src/cli.rs:546-562, .engineering/planning/story/unknown-commit-in-seed-and-retries.md:225-226, .engineering/waves/wave-35.md:82

What I read: 3 stories and the wave page. I ran `aep plan artifact show` on each, plus `aep plan artifact graph`, `git grep` and the `Taskfile.yml` generate and check tasks (`generate:observe`, `check:the_generated_types_have_no_drift`). I did not recheck CHANGELOG, `observe.yaml` or the generated files: the wave page already names them as shared, and I found no more shared surface there.

Surfaces established: 3 cited, 0 inferred only, 0 unplaceable. Most files are cited, a few inferred. Acceptance reads traced to a producer in the set: 1 (U3 reads U2's regenerated files). 0 of those 1 are recorded by a `depends_on` edge.

What I could not establish:
- Whether U3's implementor will edit the `--seed` help in `cli.rs`. The body is silent on it.
- No placement and plan-compliance problem in the other lanes, which are outside my lane: test budgets, no network in tests, no Python, no PostgreSQL-lane test and public-repo hygiene. I did not review these and they did not affect my verdict.

```findings
- file: .engineering/planning/story/unknown-commit-in-seed-and-retries.md
  line: 118
  category: parallel-safety
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "its acceptance reads the generated schema, Rust and TypeScript files that story:observe-label-vocabulary regenerates, and no depends_on edge runs from this story to it (the wave page says none inside the wave), so one wave would schedule both at once; add the edge or merge the two"
- file: crates/spindle-cli/src/cli.rs
  line: 546
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the --seed help in cli.rs (and its generated copy website/docs/reference/cli.md:153) says a rejected entry 'did not run' and that a restart sends rejected entries again, which D5 A and D6 A of story:unknown-commit-in-seed-and-retries change; its scope does not name cli.rs or cli.md, so the story must say it leaves them alone or admit the shared file with story:link-bound-setting (which edits cli.rs and cli.md) and order the two or split the surface (inferred surface)"
```
