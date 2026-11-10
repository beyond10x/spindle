---
format: aep.planning-md/3
id: review-result:w17-u5-adversary-1
kind: review-result
status: active
title: U5 store reset and world, adversary pass 1
relations:
- reviews: story:store-reset-and-world-specified
revision: 1
---
unit: U5 story:store-reset-and-world-specified, branch unit/w17-world, head c4d2dc2c plus my 4 uncommitted test paths
verdict: NEEDS-CHANGE
cases: executed 317→322, red 5
origin: introduced 6 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 path (directory tree, listed in part 6)
needs-coordinator: 8 failures in the spindle-cli suite are not my cases (5 conform::*, 3 that need the UI built); I did not attack them

The specification does not agree with the code on five points, and each one has a red case.
`origin: introduced` is because no code changed between e3a73af0 and c4d2dc2c. The code behaves
exactly as it did at the base, but `ess/domains/console.yaml` and its schema exist only from
c4d2dc2c, so every disagreement starts with this unit.

**1. Diff.** `crates/spindle-cli/tests/spindle-cli/main.rs` and
`crates/spindle-ingress/tests/spindle-ingress/main.rs` +1 each; new untracked
`crates/spindle-cli/tests/spindle-cli/adversary_world_spec.rs` and
`crates/spindle-ingress/tests/spindle-ingress/adversary_component_reset.rs`. Test files only.

**2. Cases added, all five red now (exit 101).**

- `adversary_world_spec::a_world_the_schema_refuses_for_its_shape_is_refused` (`:142`): a body that
  is an array, a row that is an array, `{"mint":null}` and `{"decisions":null}` each answer
  `200 {"emptied":["library"]}`.
- `adversary_world_spec::a_decision_table_outside_the_specified_shape_is_refused` (`:166`): nested
  tables `{"decisions":{"shelf":{"books":{"Donate":{"refused":true}}}}}` and `{"decisions":{"nowhere":{}}}`
  each answer 200.
- `adversary_world_spec::a_mint_the_schema_accepts_is_loaded` (`:189`): `{"mint":7.0}` and
  `{"mint":1e1}` answer `400 {"refused":"reading the reset's world: invalid type: floating point
  `7.0`, expected u64"}`.
- `adversary_world_spec::a_large_world_is_answered_a_specified_status` (`:204`): `{"mint":7}` plus
  1 MiB of whitespace answers 413 `{"refused":"the body is larger than 1048576 bytes"}`.
- `adversary_component_reset::a_component_ingress_refuses_a_reset_with_a_world_400`
  (`adversary_component_reset.rs:60`): the surface `play --component` builds (`component.rs:258`)
  answers `POST /api/reset` with `{"mint":7}` 404 `{"refused":"no route serves /api/reset"}`, not
  400.

Schema side: the implementor's `run.sh` extended with 12 `adv-*` fixtures ends `result: red`, exit
1. Code accepts, schema refuses: body as array, row as array, `mint: null`, `decisions: null`. Code
refuses, schema accepts: integral float mint, exponent mint, a lower-case state (not raised).

**3. Suite runs.** `cargo test -p spindle-ingress --test spindle-ingress`: `28 passed; 1 failed`
(mine). `cargo test -p spindle-cli --test spindle-cli`: `281 passed; 12 failed; 4 ignored`. Four
failures are mine. The other eight: `conform::*` (5) `ESS-SYNTH-003 … spindle.deployment.Deployment
has no scenario …`; `deploy::the_ui_is_served_from_the_binary` and `observe_panel::*` (2) need
`ui/dist` built. Executed 317 → 322.

**4. Findings** (tree c4d2dc2c)

- F1 `ess/domains/console.yaml:76,88`: the schema refuses `"mint": null` and `"decisions": null`;
  the run accepts them (`world.rs:35-36`). Fix: `presence: omitted_or_null` (ess/31), or refuse
  null in code. NEEDS-CHANGE, introduced.
- F2 `ess/domains/console.yaml:55`: the run accepts a World or a row written as a JSON array (serde
  fills a struct from a sequence, `world.rs:30-47`); the schema says object. Nothing found reaches
  it. INFEASIBLE, introduced.
- F3 `ess/domains/console.yaml:85`: the `decisions` summary and UNMAPPED say any other value is
  refused; the run joins nested tables into a command name and never checks an empty table
  (`spindle-host/src/decisions.rs:212-240`). NEEDS-CHANGE, introduced.
- F4 `ess/domains/console.yaml:88`: `mint` 7.0 and 1e1 validate, the run refuses them 400 (u64).
  CONFIRMED, introduced.
- F5 `ess/domains/console.yaml:17-22`: statuses listed as 200/400/500; a body over 1 MiB answers 413
  (`router.rs:142-147`). CONFIRMED, introduced.
- F6 `ess/domains/console.yaml:63-64`: "`play --component` … refuses a reset with a body `400`" is
  wrong; it serves no console route (`component.rs:258`), so every reset gets 404. NEEDS-CHANGE,
  introduced.

**5. Attacked and not broken.** ResetAnswer and ResetRefusal bodies and closed schemas;
`HostError::World` → 400, others 500; a refused world changes nothing; `emptied` in name order;
unknown keys refused at both levels; `rows: null`/`fields: null` refused by both sides; cited line
ranges match.

**6. Outside the worktree.** `…/w17/world/adv/` (cases.sh, suite.sh, schema-run.log, check/, logs/).
Lease `world-adv` released.

```findings
- file: ess/domains/console.yaml
  line: 76
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the World schema refuses "mint" null and "decisions" null, but the run accepts both and answers 200
- file: ess/domains/console.yaml
  line: 55
  category: contract-drift
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the run accepts a World body or a WorldRow written as a JSON array, because serde fills a struct from a sequence, while the schema requires an object
- file: ess/domains/console.yaml
  line: 85
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the decisions summary and UNMAPPED comment promise a refusal the run does not make, since nested tables are joined into a command name and empty tables under any key load with 200
- file: ess/domains/console.yaml
  line: 88
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the schema accepts the integral mints 7.0 and 1e1, but the run refuses them 400 because it reads mint as u64
- file: ess/domains/console.yaml
  line: 17
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the reset's statuses are listed as 200, 400 and 500, but a World body over 1 MiB is answered 413 with a refused body
- file: ess/domains/console.yaml
  line: 63
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the World summary says play --component refuses a reset with a body 400, but play --component serves no console route and answers every reset 404
```
