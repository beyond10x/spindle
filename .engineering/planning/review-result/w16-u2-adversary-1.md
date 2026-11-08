---
format: aep.planning-md/3
id: review-result:w16-u2-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on external-answers-observed (wave 16)
relations:
- reviews: story:external-answers-observed
revision: 1
---
needs-change

Adversary pass 1 on story:external-answers-observed (wave 16 U2), attacking cacc49b0. Seven cases
in `crates/spindle-cli/tests/spindle-cli/adversary_external_answers.rs` (commit bdccf58b); 2 red
on cacc49b0.

- F1, warning, introduced, fixed in the unit (4dfaf444): a cause asked by a command an event binding runs
  inside a served request was counted in the metric and listed in no request record; the router
  kept a request's causes by the routed command's name.
- F2, warning, introduced, fixed in the unit (4dfaf444): a same-named command a binding reinvokes added its
  causes to the routed request's externals, listing an outcome twice under one name.
- F3, note, introduced, fixed in the unit (4dfaf444): causes asked outside a request accumulated until the
  next request drained them.

Ruling: a request's `externals` lists every cause asked while it is handled, bound commands
included, each naming the command that asked it (`spindle.observe.ExternalAnswer` gains a
required `command` field, specification first); causes asked outside a request are counted and
not kept. The two red cases' assertions were adjusted only where they conflict with the ruling
(the added `command` field, the bound command's cause now listed).

All 7 cases green on 4dfaf444; merged into wave 16 as cd444acf.

```findings
[
{"file":"crates/spindle-ingress/src/router.rs","line":242,"category":"acceptance","severity":"warning","verdict":"CONFIRMED","origin":"introduced","message":"a cause asked by a command an event binding runs inside a served request is counted and listed in no request record"},
{"file":"crates/spindle-ingress/src/router.rs","line":242,"category":"acceptance","severity":"warning","verdict":"CONFIRMED","origin":"introduced","message":"a same-named command a binding reinvokes adds its causes to the routed request's externals"},
{"file":"crates/spindle-ingress/src/observe.rs","line":556,"category":"judgement","severity":"note","verdict":"CONFIRMED","origin":"introduced","message":"causes asked outside a request accumulate until the next request drains them"}
]
```
