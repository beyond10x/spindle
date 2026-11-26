---
format: aep.planning-md/3
id: review-result:wave26-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 26 U4
relations:
- reviews: story:scenario-act-without-outcome
- reviews: story:scenario-without-components
revision: 1
---
needs-revision

```
unit: story:scenario-act-without-outcome, story:scenario-without-components
verdict: CONFIRMED
cases: executed 26→35, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
```

Red (`crates/spindle-cli/tests/spindle-cli/adversary_w26_u4.rs`): a specification without components
refused at admission printed `implied: store.durable`, naming a component the specification does
not declare, where a stdin run names none.

Fixed in 88231907a: a refused replay prints the JSON a stdin run prints,
`{"missing":[...],"outcome":"refused"}`, exit 3. The case is green.

```findings
- file: crates/spindle-cli/src/replay.rs
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a specification without components refused at admission named the implied component, where stdin mode names none
```
