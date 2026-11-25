---
format: aep.planning-md/3
id: review-result:node-export-fixed-actor-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on the Node export fixed actor story
relations:
- reviews: story:node-export-fixed-actor
revision: 1
---
approve

Both acceptance lines name an observable outcome. `a fixed actor sends each command as run does` compares the Node lane `check:node_serves_a_specification` with answers recorded from the native loopback listener under the same `--fixed-actor`/`--fixed-caller`; `create refuses an undeclared fixed actor` compares the refusal with the reason `run` prints. The tests carry both names (`crates/spindle-node/tests/node/fixed-actor.test.mjs`, `crates/spindle-node/tests/spindle-node/fixed_actor.rs`). The Outcome also names a caller its binding refuses, with no acceptance line of its own; `fixed-actor.test.mjs` tests it.

```findings
[]
```
