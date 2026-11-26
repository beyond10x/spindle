---
format: aep.planning-md/3
id: review-result:wave26-des-r2
kind: review-result
status: active
title: Plan critic (design), round 2, on wave 26
relations:
- reviews: story:related-selector-rows-quantifier
- reviews: story:guard-over-absent-row-field
- reviews: story:view-filter-parameter-literal
- reviews: story:run-fails-on-rejected-line
- reviews: story:clock-and-mint-in-every-mode
- reviews: story:scenario-act-without-outcome
- reviews: story:scenario-without-components
- reviews: story:release-linux-glibc-floor-and-musl
revision: 1
---
Those are generic. Now read the stories.
approve

Round 1: the design critic's three findings are all resolved. The two missing `depends_on` edges (`story:clock-and-mint-in-every-mode` → `story:run-fails-on-rejected-line`, `story:scenario-without-components` → `story:scenario-act-without-outcome`) are now in `aep plan artifact graph`. The release story no longer names a worktree path, a branch or the sibling repositories' script paths. A grep of all 8 bodies for home paths, the user's name, an email, a company name or an account name found nothing. The only hit for "organization" or "organisation" is generic, in the stories' reproduction text.

**What you read.** I read 8 stories in full, the wave page, `review-result:wave26-des-r1`, and the commit `d7f224007` stat. I ran `aep plan artifact relations`, `graph` and `validate`. `validate` printed `valid` plus old unassessed review-results from earlier waves, which are not findings. I walked the whole-store graph filtered to the set's ids, about 50 edge lines, including edges outside the set. There is no cycle. The only needs-first edges are the two above, so there is no serialising chain. I traced 2 acceptance reads of state a sibling produces, and both have a `depends_on` edge. The first is the earlier-`at` rejection and its exit status, produced by the run-fails story. The second is the act-line outcome compared with stdin, produced by the act-outcome story.

**What I could not establish.**
- U1: `story:guard-over-absent-row-field` shares `execute.rs`, `lib.rs` and the kernel `main.rs` with `story:related-selector-rows-quantifier` but reads no state it produces. Order by edge or by split surface is the parallel-safety critic's call (out of my lane).
- U3 and U4 both touch `HostTarget` in `crates/spindle-conformance/src/target.rs`: U3 adds a separate constructor, U4 adds a field, and the coordinator joins them at merge. Both stories write this down, so I treated it as shared-file territory and not as a split abstraction. Whether it can run at once is the parallel-safety critic's call (out of my lane).
- Out of my lane, for the acceptance critic: the `## Outcome` of `story:guard-over-absent-row-field` still promises that the kernel selects ESS's outcome instead of failing as `undecidable`. Its Decided section says ESS is `undecidable` there too, and acceptance 1 and 2 are expected to pass already.
- The `zigbuild`, `auditable` and glibc claims in the release story are about a workflow I did not run.

```findings
[]
```
