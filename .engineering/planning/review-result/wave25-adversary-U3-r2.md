---
format: aep.planning-md/3
id: review-result:wave25-adversary-U3-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 25 U3
relations:
- reviews: story:identity-mint-types-and-fallback
revision: 1
---
needs-revision

```
unit: story:identity-mint-types-and-fallback, head 6404b9f9c
verdict: NEEDS-CHANGE
cases: executed 813→825, red 5
origin: introduced 2 / pre-existing 1 / undecided 1
```

Red on 6404b9f9c:

- A minted Integer `n` is not drawn again where an instance holds `n.0`, and a second instance under
  the same Integer is created (kernel and HTTP); two creations sending `0.0` and `0` make two
  instances where ESS refuses the second. Root: the store keys an identity by its JSON spelling
  (`store.rs:565`), pre-existing; decided to belong to story:integer-identity-any-spelling, and the
  three cases leave the unit.
- With 8 instances held under the next 8 draws the kernel refuses after 8 draws where the
  interpreter opens on the 9th (kernel and HTTP): ESS bounds the redraw by the held population plus
  one.

Attacked and not broken: the redraw keeps the token, the one-time ticket and `sets:` copies on the
minted and the fallback path; an Integer newtype identity is drawn again; an Integer held by another
entity causes no redraw; a reopened SQLite store; the seeded skip moves later draws by exactly one.

Decided by the coordinator: bound the redraw as ESS does; fix the misplaced doc of
`determined_identity`; an exhausted redraw answered 501 stays.

```findings
- file: crates/spindle-kernel/src/execute.rs
  line: 1874
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "the redraw's held check matches by JSON spelling, so a minted n is not drawn again where an instance holds n.0"
- file: crates/spindle-kernel/src/store.rs
  line: 565
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "identities are keyed by JSON text, so creations sending 5.0 and 5 make two instances"
- file: crates/spindle-kernel/src/mint.rs
  line: 159
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "a fixed bound of 8 draws refuses a creation the interpreter opens"
- file: crates/spindle-ingress/src/answer.rs
  line: 531
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: undecided
  message: "an exhausted Integer redraw is answered 501"
- file: crates/spindle-kernel/src/execute.rs
  line: 1833
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: "determined_identity's doc sentence heads creation_identity's doc"
```
