---
format: aep.planning-md/3
id: story:when-related-over-reference-list
kind: story
status: implemented
title: 'A when_related: guard over a list of references is decided'
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:51Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-24T02:29:33Z", actor: "human:timo", revision: 3, decided_on: {"recorded":{"test_result":1}}}
- {from: "active", to: "implemented", at: "2026-11-24T02:29:34Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# A `when_related:` guard over a list of references is decided

## Outcome

A command whose branch guards with `when_related:` through a field of type `List<…Ref>` (each
element names a row of the related entity) is run: the kernel reads every referenced row and
decides the guard as ESS's interpreter decides it, instead of refusing the command. Today
`crates/spindle-kernel/src/supported.rs:288-291` refuses it as "`when_related:` over a list of
references".

## Acceptance

- `a_guard_over_a_list_of_references_is_decided_as_the_interpreter`: a neutral fixture with a
  command whose `when_related:` goes through a `List` of references, sent with every referenced row
  present, with one missing, and with an empty list, answers the outcome ESS's interpreter answers
  for each (`ess-conformance` at the pinned tag).
- `conform` on the fixture executes the scenarios ESS synthesises for the command and they pass;
  `conformance.toml` of the repository is unchanged.
- The consumer measurement below reports 44 of 44 commands run and no scenario stopped by this
  construct.

## Origin

Goal story. The consumer serving its IAM specification on Spindle re-ran its measurement
(`task spindle-rerun`, its tool `tools/spindle-rerun`) against b10x-spindle 0.18.0 on 2026-11-22:
43 of 44 commands run, 18 of 20 scenarios replay; the one command not run is `DefineRole`, first
unsupported construct "`when_related:` over a list of references", and it stops the two scenarios
that do not replay (`define-role`, `role-actors`).
