---
format: aep.planning-md/3
id: review-result:wave35-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 35
relations:
- reviews: story:link-bound-setting
- reviews: story:observe-label-vocabulary
- reviews: story:unknown-commit-in-seed-and-retries
revision: 1
---
needs-revision

- story:unknown-commit-in-seed-and-retries — its acceptance reads state U2 produces and no edge records it, so this is a trade-off. Choose one of two options.
  - **Option 1:** add `depends_on story:observe-label-vocabulary`. `aep plan artifact waves` then orders U3 after U2, which matches the merge order the wave page already states. The wave page line "No `depends_on` edge inside the wave" would then change.
  - **Option 2:** split the shared surface, as U2's D4 C and U3's D6 C already describe. U3 leaves `ess/domains/observe.yaml` and the generated files, so the two no longer collide.
  - **Evidence:** the acceptance line "`task check:the_generated_types_have_no_drift` pass[es] … regenerated on top of `story:observe-label-vocabulary`'s merged change" cannot be checked until U2 lands. The graph has no edge between the two, and U2's own D4 text records the order only in prose. Citation: .engineering/planning/story/unknown-commit-in-seed-and-retries.md:118-119, .engineering/planning/story/observe-label-vocabulary.md:112, .engineering/waves/wave-35.md:59.

I found no cycle, no serialising chain, no split abstraction and no horizontal slice.

- **U1:** it is self-contained and reads nothing a sibling produces.
- **U2:** its fixture, `FAMILIES` and generated schema all sit in one unit, so it is one abstraction.
- **U3:** the bound-command and seed halves share the one `StoreError::Unknown` rule and the dead-letter marker, and I judge that a single outcome.

The plan meets the rules you listed.

- **Test time:** the stated time is about 1.3 s added by U3, one new test under 1 s in U1, and one changed unit test in U2. No test touches the PostgreSQL lane.
- **Python:** none is added.
- **Network and model calls:** the wave page's "What a unit must not do" forbids both.
- **Privacy:** a grep of the four files for home paths, names and email addresses found nothing.

**What I read:** 3 stories and the wave page, using `aep plan artifact show` on each, plus `relations`, `graph`, `validate` (it prints `valid`) and `waves`. I walked about 14 declared edges, including the 8 drafts that depend on U2, the epic that U2 decomposes, and the wave-34 story, so I went outside the set. I traced 1 acceptance read of a sibling's output to a producer in the set (U3 reading U2's regenerated files), and 0 of those are recorded by an edge.

**What I could not establish:**
- **Wave-34 dependency:** U3's scope says it depends on `story:unknown-commit-answered-as-unknown`, an active wave-34 story, and the graph has no edge to it. It lies outside the set and the tree at b546d6d2 already holds `StoreError::Unknown`, so I did not count it.
- **Out of my lane:**
  - The scoper's open point on whether the seed unit test needs the `notebooks` fixture belongs to scope or acceptance.
  - Whether U2's vocabulary and label rule have an acceptance line that checks them belongs to acceptance. The acceptance names only `task check:the_observe_domain_validates`, and I did not rule on it.

```findings
- file: .engineering/planning/story/unknown-commit-in-seed-and-retries.md
  line: 118
  category: design
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "its acceptance reads the generated files that story:observe-label-vocabulary's merged change produces and no depends_on edge records it; either add depends_on to story:observe-label-vocabulary (and change the wave page's 'No depends_on edge inside the wave') or split the shared ess/domains/observe.yaml surface per D6 C so U3 leaves it"
```
