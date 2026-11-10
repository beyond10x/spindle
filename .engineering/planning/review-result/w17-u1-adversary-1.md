---
format: aep.planning-md/3
id: review-result:w17-u1-adversary-1
kind: review-result
status: active
title: U1 admission decision, adversary pass 1
relations:
- reviews: story:admission-decision-checked-by-model
revision: 1
---
unit: story:admission-decision-checked-by-model, tree spindle-w17-admission at da3e25eb plus my two test-file changes (not committed)
verdict: NEEDS-CHANGE
cases: executed 228→230, red 1
origin: introduced 2 / pre-existing 1 / undecided 0
wrote-outside-worktree: 1 directory (the assigned scratch directory, listed in part 6)
needs-coordinator: regenerate the generated schemas and types, re-pin `conformance.toml` and `conformance-known-failures.json` (5 `conform::*` CLI tests fail until then), and decide finding 2 (pre-existing, in the story's scope)

The UI is not built in this tree. That makes 3 CLI tests fail for a reason unrelated to the unit.

**1. Diff.** `crates/spindle-kernel/tests/spindle-kernel/main.rs` +1; new untracked
`crates/spindle-kernel/tests/spindle-kernel/adversary_admission_checked.rs`. Test files only.

**2. Cases added** (`cargo test -p spindle-kernel --test spindle-kernel adversary_admission_checked::`, exit 101)
- `adversary_a_refusal_of_a_capability_the_environment_provides_is_not_refused`: red now. `laptop`
  provides `store.durable` by `sqlite`; the decision requires `[store.durable]`, binds `[]`, names
  `[store.durable]` missing, and the cause answers yes. Red:
  `assertion `left != right` failed: `laptop` provides `store.durable`, and the model refused it as
  missing: Executed { outcome: "refused", … }` — `test result: FAILED. 1 passed; 1 failed`.
- `adversary_admitted_and_refused_answer_only_a_valid_decision`: green now. A fixed-seed property
  test, 400 random decisions over 3 capabilities and 3 implementations: `admitted` only for a valid
  decision with nothing missing, `refused` only for a valid decision with something missing, every
  other answer an error leaving the deployment `Offered`; all five outcomes reached.

**3. Suite runs.** Kernel binary: `229 passed; 1 failed` (mine); without my module `228 passed`.
CLI binary: `281 passed; 8 failed; 4 ignored`: five `conform::*` fail on the known-failures
declaration until the re-pin, three need `ui/dist`. `ess generate --path ess --kind schema … --check`
exits 1 on head (34 files differ, `errors/spindle.deployment.EnvironmentUnknown.schema.json`
missing) and 0 on e3a73af0; the Rust and TypeScript checks also exit 1 on head. Synthesis:
`66 scenario(s) (0 authored), 2 refusal(s)` (base `56 … 4`); both remaining are
`DecideAdmission/refused`, "such a reference is not arranged for an external branch".
`ess verify conform run --target interpreted`: 65 passed, 1 failed (the known
`LaunchInstance/outcome/not-admitted`). `b10x-spindle conform … --counts conformance.toml`: 66
executed, 65 passed, 1 failed, 0 skipped; counts moved only upward. 12 hand-written edge scenarios
pass on both the interpreter and `b10x-spindle conform`.

**4. Findings** (da3e25eb)

1. `ess/generated/jsonschema/…/spindle.deployment.DecideAdmission.schema.json`: NEEDS-CHANGE,
   introduced. Generated schemas and types were not regenerated; fix `task generate:observe`.
2. `ess/domains/deployment.yaml:304`: CONFIRMED, pre-existing. A refusal naming missing a capability
   the environment provides is answered `refused`; the interpreter agrees, so it is the model's
   behaviour. No runtime path sends `DecideAdmission` (design §3a); the story's outcome names this
   case.
3. `docs/design/spindle.md:135`: CONFIRMED, introduced. The two ESS limits the doc cites do not
   rule out a separate `missing-provided` guard branch (`when_related` via `environment_id`,
   answering `AdmissionDecisionInvalid`). A scratch copy validates, the interpreter runs 65 of 66
   synthesised and 12 of 12 hand-written scenarios, and synthesis has 3 refusals, one more.

**5. Attacked and not broken.** An unprovided binding is never admitted (property test, s2, s4, s8);
an invalid decision is never refused (property test, s9); s3, s5, s10-s14 agree with the
interpreter. `crates/spindle-control/src/control.rs` sends neither command (it admits through
`EnvironmentAdmission::decide`, line 274); every other sender is a test using the new shape.

**6. Outside the worktree.** `…/w17/admission/adv/` (scenario, suite, report and log files).

```findings
- file: ess/generated/jsonschema/schema/commands/spindle.deployment.DecideAdmission.schema.json
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the committed generated schemas and observe types were not regenerated after the deployment.yaml change, so the generated-types drift check exits 1 on head where it exits 0 on the base
- file: ess/domains/deployment.yaml
  line: 304
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: refused accepts a refusal naming missing a capability the deployment's environment provides, which the model could check from the Environment row it already reads for not-provided
- file: docs/design/spindle.md
  line: 135
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the stated ESS limits do not rule out a separate missing-provided guard, which validates and runs correctly on the 0.90 interpreter at the cost of one more synthesis refusal
```
