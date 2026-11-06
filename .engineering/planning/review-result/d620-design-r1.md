---
format: aep.planning-md/3
id: review-result:d620-design-r1
kind: review-result
status: active
title: D620 stories, design critic, round 1
relations:
- reviews: story:response-field-value-source
- reviews: story:http-outcome-bodies
revision: 1
---
approve

No design findings. The two stories are cut along the kernel/ingress seam, so each can be shown working alone.

**Shape**
- **Split:** `story:response-field-value-source` touches `crates/spindle-kernel/src/{supported,values}.rs`. `story:http-outcome-bodies` touches `crates/spindle-ingress/src/{answer,surface}.rs`. Neither describes half of the other's abstraction.
- **Value source:** it lets a payload or `sets:` read `response.<field>`. The kernel already answers response fields, and the stories only meet at the command's response.
- **HTTP bodies:** it serves a binding's declared body, encoding, headers and redirect. Its acceptance reads the route's answer and `ess generate --kind openapi`, neither of which the sibling produces.
- **Hidden dependencies:** none.
  - `story:response-field-value-source` reads the response that `story:structured-response-values` (active) extends. It declares `depends_on` on it, which matches.
  - `story:http-outcome-bodies` reads only the command-response serving that `story:kernel-command-responses` (implemented) already landed. It needs no edge, and it declares none.
- **Chain, cycle, horizontal slice:** none. Neither story is a layer cut. Each ends in an observable answer.

**Claims checked against code**
- **Refusal text:** `crates/spindle-ingress/src/surface.rs:851-858` refuses `Body != Envelope`, a non-default encoding, headers or a redirect with "answered other than with its status and the envelope". That matches the story text.
- **Value-source refusal:** `crates/spindle-kernel/src/values.rs:146` and `supported.rs:282` produce "the value source {describe}" for any source the kernel does not read. `ResponseField` appears nowhere in spindle, so it falls into that arm. The story's claim holds.
- **ESS citations:**
  - `determined.rs:419` is `ResolvedPayloadValue::ResponseField`.
  - `execute.rs:2610` is its dispatch to `determined::response`.
  - `ess-http/src/lib.rs:490` is the `Body` enum.
  - `docs/design/http-presentation-binding.md` exists.

**What I read**
- Both story bodies, `story:structured-response-values` and `story:kernel-command-responses`.
- `aep plan artifact relations`, `graph` and `validate` (valid, 148 artifacts).
- The code citations above in `spindle-wave10` and ESS `bd6ecb3`.

**Could not establish**
- **Scope of `story:response-field-value-source`:** its scope lists only `supported.rs` and `values.rs`. Reading the response during event and row construction may also need `execute.rs` or `mint.rs` (the scope of `story:structured-response-values`). That is the scope and parallel-safety critics' lane.
- **Acceptance wording:** whether "names the mint the response field needs and no other" can be checked is the acceptance critic's lane.
- **Edges:** I walked 7 edges, including edges to artifacts outside the set, and found no cycle. I traced 2 acceptance reads to producers, and both are recorded or already implemented.
- **Not run:** I did not check the 62b97a9 origin claim or the "8 calls" and "6 calls" counts, and I ran no tests.

```findings
[]
```
