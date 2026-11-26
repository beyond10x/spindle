---
format: aep.planning-md/3
id: review-result:wave25-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 25 U3
relations:
- reviews: story:identity-mint-types-and-fallback
revision: 1
---
needs-revision

```
unit: story:identity-mint-types-and-fallback, head 0afa7aa7f
verdict: NEEDS-CHANGE
cases: executed 753→766, red 3
origin: introduced 2 / pre-existing 1 / undecided 0
```

Red on 0afa7aa7f:

- `adv_a_seeded_mint_does_not_mint_a_seeded_integer_identity`: a world reset with `mint: 7` and a
  row holding the crate id that seed draws first; the next `OpenCrate` creates a second crate under
  the same identity (`3444597933762076` beside `3444597933762076.0`).
- `adv_a_seeded_integer_row_is_addressed_by_its_integer_input`: a world row with Integer identity 42
  is not found by `RenameCrate {"crate_id": 42}` (base the same; story:world-row-integer-fields
  writes it as an integer).
- `adv_two_fresh_uuids_sharing_their_low_bits_do_not_refuse_a_creation`: two fresh UUIDs with equal
  low 53 bits mint one Integer and the creation is refused where the interpreter draws again; only
  reached with a constructed mint.

Attacked and not broken: the minted Integer stays in [0, 2^53-1]; 200 random creates give 200 ids;
a minted id as later input in the kernel and over HTTP; an Integer newtype with an invariant stays
refused; seeded avoidance of a bare String id; every fallback form against ESS's interpreter,
explicit `null` and nested paths included; `minted` and `derive.rs` unchanged.

Decided by the coordinator: wave/25 is merged into the unit for the world-row fix; the held set of
a seeded mint takes Integer identities; a minted Integer an instance holds is drawn again, bounded.

```findings
- file: crates/spindle-host/src/host.rs
  line: 1217
  category: concurrency
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "Seeded::avoiding holds only UUID-text identities, so a world reloaded with its own mint seed re-mints a seeded Integer identity"
- file: crates/spindle-host/src/world.rs
  line: 116
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: "a world row's Integer identity is written as 42.0 and keyed apart from the integer 42 a request sends"
- file: crates/spindle-kernel/src/mint.rs
  line: 381
  category: property
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: "two fresh UUIDs sharing their low 53 bits mint one Integer identity and the creation is refused where the interpreter redraws"
```
