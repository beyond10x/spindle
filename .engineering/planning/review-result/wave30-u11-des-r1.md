---
format: aep.planning-md/3
id: review-result:wave30-u11-des-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
approve

**What I read:** all of `story:affects-over-input-list`, plus `story:decimal-identity-any-spelling`, `story:create-on-held-identity-unavailable` and `story:rally-example-reads-stdin`. I checked the scope and relation lines of the other nine stories, and the wave page's units table, Surfaces section and merge order. Commands: `aep plan artifact show`, `relations`, `graph` (dot, 1995 edges), `validate` (it printed `valid`) and `waves`. In the tree I read `supported.rs:560-660`, `execute/affects.rs:1-330`, `facts.rs:195-220`, `outcome_scoped_refusal.rs:500-620` and the fixture `lot.yaml`. I also checked `ResolvedEach` and `ResolvedElementRead` in the pinned ESS checkout (`d7833ee`).

**Edges walked:**
- U11 has two edges out, `serves vision:spindle` and `depends_on story:decimal-identity-any-spelling`, and none in.
- I followed U6's edge outside the set to `story:integer-identity-any-spelling` (implemented, `informed_by`).
- U5 has no `depends_on`. No cycle, and no chain that forces the set into one order.
- `waves` puts U11 after U6.

**Acceptance reads traced:** 1 to a producer in the set, and an edge records it. The parity case where a list element spells a `Decimal` identity `1.0` for a row held as `1` needs U6's single spelling, and U11 has a `depends_on` edge to U6.

**Other reads checked:**
- The `admit` line reads the kernel's refusal set. U3's new refusal of a required `Unanswered` error field cannot reach this fixture: its only error, `LotClosedAlready`, has `fields: []`. Wave 29 only adds refusals.
- The stdin and HTTP line needs no `--stdin` flag from U9. The fixture is `reached_by: in_process` (`system.yaml:23`), and the HTTP side uses the existing `--http`.

**Shape of U11:** it is one vertical slice. Admission, the run itself, the CLI and HTTP answers, the docs and the parity test all sit in one story, and the moved stdin-refusal test is part of the same change. The new filling from list elements stays inside `affects.rs`, and `facts.rs` if needed. It does not go through `values.rs` `Sources`, which U3 and wave 29 change: ESS gives element reads a separate list of field-to-member reads (`ResolvedEach.reads`, `ir.rs:1126-1158`). So U11 and U3 don't each hold half of one design.

**What I could not establish:**
- U11 and U1 both edit `website/docs/concepts/specification.md` (U11 at `:124-125`, U1 at `:168-173`), and the wave page's Surfaces section does not list that file as shared. That belongs to the parallel-safety critic, not me.
- The rule "an element naming the subject is skipped" compares the element's identity with the subject's. The story gives the single spelling only to an element's identity read from inside the list, and does not say the subject's identity gets the same spelling for that comparison. That is a parity question for the acceptance or scope critic.
- The "construct the kernel still refuses and ESS accepts" for the moved refusal test is not named. Outcome-level candidates that stay refused after wave 29 and wave 30: `compensates: true`, and `sets:` without a subject. Whether ESS accepts them is for the acceptance critic.

```findings
[]
```
