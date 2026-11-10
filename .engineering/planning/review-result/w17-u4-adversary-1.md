---
format: aep.planning-md/3
id: review-result:w17-u4-adversary-1
kind: review-result
status: active
title: U4 environment implementations, adversary pass 1
relations:
- reviews: story:environment-implementations-specified
revision: 2
---
unit: story:environment-implementations-specified, tree spindle-w17-environment at c2f2b782 (base 582f57d3) plus my two test-file changes (not committed)
verdict: NEEDS-CHANGE
cases: executed 232→238, red 1
origin: introduced 2 / pre-existing 1 / undecided 0
wrote-outside-worktree: 1 directory (the assigned scratch directory, listed in part 6), plus this report
needs-coordinator: finding 3 is red on wave/17 itself (f99b98f5 too), outside this unit's diff; decide who adds `domains/console.yaml` to the load test

wave/17 has moved to f99b98f5 since the brief. Every base read here uses 582f57d3.

**1. Diff.** `crates/spindle-kernel/tests/spindle-kernel/main.rs` +1; new untracked
`crates/spindle-kernel/tests/spindle-kernel/adversary_environment.rs`. Test files only.

**2. Cases added** (`cargo test -p spindle-kernel --test spindle-kernel adversary_environment::`, exit 101, `5 passed; 1 failed; 232 filtered out`)
- `adversary_a_binding_no_key_covers_is_not_admitted_beside_a_prefix_key` (:235): red now. The test declares
  `examples/environments/laptop.toml` as written, one Provision per key, so `external.*` is a
  prefix key. The runtime gives `implementations("net.http") == None`. The model admits `net.http`
  bound to `tcp`. Red:
  `assertion `left != right` failed: laptop.toml provides no `net.http`, and the model admitted it bound to `tcp`: Executed { outcome: "admitted", events: [Event { name: "spindle.deployment.DeploymentAdmitted", … }], error: None, response: None }`
- `adversary_a_key_listed_twice_is_key_twice` (:131), green: a key given twice, with the same or
  other implementations and as a prefix or not, is `key-twice`, `ProvidesInvalid`, no event, no row.
- `adversary_a_key_with_no_implementation_is_no_implementation` (:158), green: an empty list,
  alone or last on a prefix key, is `no-implementation` and records nothing.
- `adversary_keys_the_document_parser_accepts_are_declared` (:182), green: an empty `provides`,
  `store.*` beside `store.durable`, `*`, `.*`, `store.` and `""` all parse in
  `Environment::parse`, and the model answers `declared` for each.
- `adversary_a_key_that_is_not_a_prefix_does_not_disable_not_provided` (:214), green: for `*`,
  `store*`, `store.` and `store.**` the runtime finds nothing for `store.durable`, and the model
  answers `not-provided`.
- `adversary_the_model_never_refuses_what_the_runtime_provides` (:284), green: a fixed-seed
  property over 300 rounds of random documents (exact and prefix keys, nested prefixes, `*`, `.*`,
  `store.`). The runtime's own decision is always `admitted` or `refused`. A binding to any listed
  implementation, not only the first, is never `not-provided` or `missing-provided`.
- Mutants in scratch copies of `ess/`, run via `ADVERSARY_ESS_ROOT` (`mutants.log`). With
  `key-twice` dropped, :131 goes red (`left: "declared" right: "key-twice"`). With
  `no-implementation` dropped, :158 goes red. With `ends_with ".*"` changed to `"*"`, :214 goes red
  (`*: … left: "admitted" right: "not-provided"`). No existing kernel case sends either invalid
  `provides`. The synthesised suite does cover both outcomes, but it is unpinned for now.

**3. Suite runs** (after part 2)
- Kernel binary: `237 passed; 1 failed` (mine), exit 101. With `--skip adversary_environment::`:
  `232 passed; 6 filtered out`.
- `-p spindle-capability -p spindle-spec`: `load::spindles_own_specification_compiles` FAILED
  (`left: [… "domains/environment.yaml", "domains/console.yaml"]`, `right: [… "domains/environment.yaml"]`), exit 101 (finding 3).
- `-p spindle-cli`: `284 passed; 9 failed; 4 ignored`, exit 101. Five are `conform::*` and three
  need `ui/dist`, all known. The ninth, `processes::a_match_between_three_processes_ends_with_a_winner`,
  passed twice when run alone, so it is load-flaky and not this unit's.
- `ess specify validate --path ess`: 7 files, valid. `ESS-ENTITY-019` count: 0 on head and 0 on base.
- `task check:the_generated_types_have_no_drift` and the schema `--check` both exit 0.
- Synthesis: `68 scenario(s) (0 authored), 3 refusal(s)`. It includes
  `DeclareEnvironment/outcome/key-twice` and `/no-implementation`.

**4. Findings** (c2f2b782)

1. `ess/domains/deployment.yaml:357`: NEEDS-CHANGE, introduced. The `admitted` summary says
   "Every required capability is bound to exactly one implementation the environment provides
   for it". That is false whenever `provides` holds any `prefix.*` key: then no capability without
   an exact key is checked, including ones no key covers (case :235). `external.*` appears in
   `laptop`, `laptop-processes` and `browser`, so of the four shipped environments only `microvm`
   gets the no-exact-key guard. The `refused` hedge at :379, "wherever no `prefix.*` key decides
   it", is wrong the same way: `external.*` does not decide `net.http`, and the binding is still
   unchecked. The UNMAPPED comment at :291 states the gap correctly. At base, `provides` was exact
   `Binding`s, so the summary held. What reaches it: no runtime path sends `DecideAdmission` (only
   `crates/spindle-capability/src/admission.rs:3` names it). Readers of the specification and the
   conformance suite do reach it. Fix: say in both summaries that, where `provides` holds a
   `prefix.*` key, a capability with no exact key is left to the runtime.
2. `docs/design/spindle.md:133`: CONFIRMED, introduced. "A capability only a `prefix.*` key could
   cover is not guarded" understates the gap in the same way: any prefix key disables the guard
   for every capability that has no exact key. The sentence before it (:130-131) states the rule
   correctly.
3. `crates/spindle-spec/tests/spindle-spec/load.rs:15`: CONFIRMED, pre-existing. The expected file
   list leaves out `domains/console.yaml`, which `ess/ess-inputs.yaml` lists. At 582f57d3 the test
   expects 5 files and `ess specify validate` on that tree loads 6. `crates/spindle-spec/src` is
   unchanged by the unit. The unit's merge edited exactly this assertion and left it red. The merge
   commit reports kernel and CLI results only. Fix: append `"domains/console.yaml"` after
   `"domains/environment.yaml"`.

**5. Attacked and not broken**
- The runtime prefix rule matches the model's `ends_with ".*"` for `*`, `store*`, `store.`,
  `store.**` and `.*` (`environment.rs:90-91`).
- No binding was refused that the runtime would provide (300-round property). Exact-wins agrees
  with the runtime.
- `CapabilityKey`, `CapabilityName` and `ImplementationName` are all unconstrained `String`, and so
  is the TOML parser's key. The two alphabets are equal.
- `key-twice` and `no-implementation` were tried on two keys, a prefix beside an exact key, an empty
  list and an empty `provides`.
- `missing-provided` agrees with `admit`: the runtime names a capability missing only when no key
  covers it (`admission.rs:46-54`), and `no-implementation` rules out empty lists.
- UNMAPPED claims at :291 and :328 hold for ESS 0.90. `starts_with: {path: p.capability}`,
  `{input: p.capability}` and `{input: b.capability}` are all `unparsable_predicate`.
  `p.capability <= b.capability` does validate, but an ordered comparison gives only a one-sided
  bound, not a prefix match.
- CHANGELOG and AGENTS.md agree with the code.

**6. Outside the worktree.** `~/.cache/scratch/a9933233-1055-4091-b7dc-91db1e33967b/w17/environment/adv/`, containing:
- scripts: `variants.py`, `mutants.py`, `run-mutants.sh`, `suite.sh`, `extra.sh`
- logs: `cases-alone.log`, `mutants.log`, `suite.log`, `extra.log`, `synth.log`
- `suite.json`
- spec copies: `ess/`, `base/ess/`, `mutant-no-key-twice/`, `mutant-no-no-implementation/`, `mutant-ends-with-star/`

Also this report, at `…/spindle-wave17/.engineering/drafts/w17/w17-u4-adversary-1.md`.

```findings
- file: ess/domains/deployment.yaml
  line: 357
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: admitted's summary promises every binding is one the environment provides, but any prefix.* key in provides leaves every capability without an exact key unchecked, so laptop.toml as written admits net.http bound to tcp
- file: docs/design/spindle.md
  line: 133
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the design doc says only a capability a prefix.* key could cover is unguarded, while the model leaves unguarded every capability without an exact key once any prefix.* key is present
- file: crates/spindle-spec/tests/spindle-spec/load.rs
  line: 15
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: the load test's expected file list omits domains/console.yaml, so it is red on head and on 582f57d3, and the unit's merge edited that assertion without making it green
```
