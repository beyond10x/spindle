---
format: aep.planning-md/3
id: review-result:wave35-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 35
relations:
- reviews: story:link-bound-setting
- reviews: story:observe-label-vocabulary
- reviews: story:unknown-commit-in-seed-and-retries
revision: 1
---
Output is large; read pieces directly.
approve

Round 1's one finding is resolved, and the revision broke nothing in the shape of the set.

- **Round-1 finding (`story:unknown-commit-in-seed-and-retries`):** it is resolved by option 1.
  - Edges: the story now carries `depends_on story:observe-label-vocabulary` and `depends_on story:link-bound-setting`. Both are in `aep plan artifact graph` and in the story's frontmatter, lines 10-11.
  - Prose: the wave page (`.engineering/waves/wave-35.md:65-70`) says "Two `depends_on` edges inside the wave", and the old line "No `depends_on` edge inside the wave" is gone. The Surfaces and Order sections give the same merge order, U1, U2, U3. U2's D4 now names the edge in the graph.
- **Cycle:** none. The two new edges point from U3 to U2 and from U3 to U1, and U1 and U2 have no edge between them.
- **Serialising chain:** none. U1 and U2 stay independent and build in parallel, and the wave page says the edges record merge order only. Each edge has its shared-file reason written beside it: the generated observe files for U2 and `cli.rs` plus `cli.md` for U1.
- **U3's new `cli.rs` and `cli.md` reads (round-1 revision):** the acceptance line (`unknown-commit-in-seed-and-retries.md:139-141`) reads `cli.md` regenerated on top of U1's change. That is the second read of a sibling's output, and its edge is recorded.
- **U2's new test and the `unit` field:** the new `the_label_vocabulary_is_the_specified_vocabulary` test reads the schema U2 itself generates. The fixture, `FAMILIES`, the schema and the test are all in U2, so the abstraction is not split. Making `unit` required adds no cross-unit read.
- **Rule checks:** the wave page's "What a unit must not do" still forbids network and model calls, Python, and PostgreSQL-lane tests. I found no person, host or home path in the stories.

**What I read:** 3 stories, the wave page, the round-1 review result, and `git show b9f8e4248` for the revised files, using `aep plan artifact show` and `graph`. The graph output was long, so I filtered it to the three stories' lines; I did not re-run `waves` or `relations` in this round. `validate` produced only warnings about earlier reviews that have no recorded outcome or findings block. Round 1 reported it as `valid`, and I did not read its final line this round. I walked about 25 graph edges, including the eight drafts that depend on U2 and the epic U2 decomposes, so I went outside the set. I traced 2 acceptance reads of a sibling's output, U3 reading U2's regenerated files and U3 reading U1's `cli.md`. Both have an edge.

**What I could not establish:**
- **Out of my lane (acceptance/scope):** U2's Scope line "Test time: … no new test" (`observe-label-vocabulary.md`, last Scope bullet) contradicts its Acceptance and the wave page, which both name one new unit test. This is a stale line, not a design defect.
- **Out of my lane (scope):** U3's Scope lists `docs/design/spindle.md`, which the wave page's Units row for U3 does not.
- **Out of my lane:** I did not check whether the generated `MetricLabel` schema gets its description from `naming.summary`, as U2's first acceptance line assumes.

```findings
[]
```
