---
format: aep.planning-md/3
id: review-result:w17-u6-adversary-1
kind: review-result
status: active
title: U6 periodic timing, adversary pass 1
relations:
- reviews: story:periodic-timing-specified
revision: 1
---
unit: story:periodic-timing-specified, unit/w17-periodic at b55f6fd9 plus one untracked test file
verdict: NEEDS-CHANGE
cases: executed 51→53 (ui vitest), red 4
origin: introduced 1 / pre-existing 0 / undecided 0 (plus 2 notes: introduced 1, undecided 1)
wrote-outside-worktree: 3 paths
needs-coordinator: none

The change breaks the UI. `deploymentShape` in `ui/src/api/shapes.ts` was not given the seven new
fields. That makes the UI typecheck fail, and `pnpm build` runs that typecheck, so `task ui`,
`task build` and `task check` fail too. Two of the UI's existing tests also go red. The fix is to
add the seven fields to the binding shape, each as `{ ...text, optional: true }`.

Case added: `ui/src/api/periodic-adversary.test.ts` (untracked), two tests, both red:
- the periodic binding of the deployment fixture has exactly the fields of Binding:
  `$.bindings[0].every: not declared` and the six others.
- the shape refuses a schedule word that is not a string: the field is refused by name first.

Suite runs: `pnpm test` exit 1, `Tests  4 failed | 49 passed (53)`; two of the four are existing
tests in `src/api/types.test.ts`. `pnpm typecheck` exit 2:
`src/api/shapes.ts(63,7): error TS2740: ... is missing the following properties from type
'Shape<Binding>': "anchor", "cadence", "every", "first", and 3 more.` At the base e3a73af0,
`vitest run src/api/types.test.ts` gave `Tests  13 passed (13)` and `vue-tsc --noEmit` exited 0.
`cargo test -p spindle-control --lib` exit 0 (4 passed); the drift and fixture checks exit 0.

Attacked and not broken: ESS spelling of every word (taken from ESS's own serializer); an event
binding carries none of the fields; ESS gives no field a default; Rust and TypeScript list the same
seven optional fields.

```findings
- file: ui/src/api/shapes.ts
  line: 59
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: deploymentShape's binding fields lack the seven schedule fields, so vue-tsc fails (TS2740, breaking pnpm build, task ui, task build and task check) and two existing UI tests plus two adversary cases are red.
- file: ess/domains/observe.yaml
  line: 80
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the seven fields are plain Optional<String>, so the published schema accepts words ESS cannot produce and schedule words on an event binding, though the summaries promise ESS's words.
- file: crates/spindle-control/src/answer.rs
  line: 184
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: undecided
  message: the words are read from a JSON value as text rather than matched exhaustively, so a reshaped ESS word would vanish without error, and cancellation and host.eligibility, which spindle-timer honours, are not published although the summary calls the seven words the keys of when.periodic.
```
