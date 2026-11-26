---
format: aep.planning-md/3
id: review-result:wave25-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 25 U4
relations:
- reviews: story:guard-over-absent-optional-input
revision: 1
---
approve

```
unit: story:guard-over-absent-optional-input, head 2e5b3d03d
verdict: CONFIRMED
cases: executed 343→354, red 3
origin: introduced 0 / pre-existing 1 / undecided 0
```

On an absent optional input the kernel and ESS 0.99.1's interpreter agree in every shape tried:
`Optional<String>`, enum, struct member (`{}`, `{"w": null}`), `Optional<List>` with `.count`,
`coalesce`, `any:`, `Optional<Decimal>`, `exists` over elements with an optional member, and
`defined()` on `Optional<Json>`, each with the key left out, `null` and present values. These 9
cases are kept in `crates/spindle-kernel/tests/spindle-kernel/adversary_w25_guard.rs`.

The 3 red cases cover a present `Optional<Map>` read by `.count` or `exists`: the kernel refuses
`undecidable` and the served route answers 501 where the interpreter selects an outcome. The unit
changes no source file, so this is pre-existing and outside the story's acceptance. The cases were
taken out of the unit and go with a backlog draft.

```findings
- file: crates/spindle-kernel/src/facts.rs
  line: 160
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "Facts::bind binds a Map input by key with no count or index facts, so a .count or exists guard over a present Optional<Map> is undecidable and the served route answers 501"
```
