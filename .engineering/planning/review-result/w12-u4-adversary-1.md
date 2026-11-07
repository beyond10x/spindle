---
format: aep.planning-md/3
id: review-result:w12-u4-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on observe-ingress-model (wave 12)
relations:
- reviews: story:observe-ingress-model
revision: 1
---
Adversary pass 1 on `story:observe-ingress-model`, wave 12 unit u4, tree at fe12f30 plus one
uncommitted test file `ui/src/api/adversary_ingress.test.ts`. Recorded from the adversary's report;
absolute scratch paths are left out (the repository is public), everything else as returned.

```
verdict: CONFIRMED (5 red cases, 2 judgement findings)
cases: executed 38→44, red 5
origin: introduced 6, pre-existing 1, undecided 0
needs-coordinator: yes. Someone must decide how a declared outcome that a binding answers with 501 or another 5xx is classed (finding 5).
```

Red cases (vitest, calling the same validator as `check:every_fixture_answer_validates_against_its_schema`):

- a request log whose ring holds more than 10 000 is refused by its schema
- an answer of another format is refused by its schema
- a routes edge counting a status class that does not exist is refused by its schema
- the request log shape refuses counts its schema refuses
- the sample shape refuses a sequence its schema refuses

`task check:ui` with the file: `Tests  5 failed | 39 passed (44)`; without it `Tests  38 passed (38)`.

Attacked and not broken: every telemetry counter key has a family; the reasons in `scripted.json`
match the router's texts; the routes edges in `tasks.json` match what the surface serves; the schema
and the shape check agree on a wrong enum, a missing or unknown member and `null`; every member the
dependent stories' acceptance reads is present.

ESS note (finding 2): in ESS 0.88.0 a string-equality invariant (`format == "spindle-requests/1"`)
validates but is only copied into `x-ess-invariants`; `validate-bundle` does not enforce it. A
one-variant enum does validate.

```findings
[
  {"file": "ess/domains/observe.yaml", "line": 386, "category": "boundary", "severity": "note", "verdict": "CONFIRMED", "origin": "introduced", "message": "RequestLog.capacity has no maximum; the schema accepts 1000000 though the run limits N to 10 000 and capacity <= 10000 projects to maximum"},
  {"file": "ess/domains/observe.yaml", "line": 385, "category": "contract-drift", "severity": "note", "verdict": "CONFIRMED", "origin": "introduced", "message": "format 'Always spindle-requests/1 / spindle-topology/1' is enforced by no projection; the schema accepts /2 (ESS 0.88.0 does not project a string-equality invariant; a one-variant enum works)"},
  {"file": "ess/domains/observe.yaml", "line": 477, "category": "boundary", "severity": "warning", "verdict": "CONFIRMED", "origin": "introduced", "message": "StatusCount.status_class is a free String; the schema accepts 6xx and ok though an enum of 2xx..5xx is expressible in ESS"},
  {"file": "ui/src/lib/conform.ts", "line": 68, "category": "contract-drift", "severity": "note", "verdict": "CONFIRMED", "origin": "pre-existing", "message": "the shape check accepts negative, fractional and out-of-range numbers the schema refuses; the new RequestLog shape inherits it"},
  {"file": "ess/domains/observe.yaml", "line": 258, "category": "contract-drift", "severity": "warning", "verdict": "CONFIRMED", "origin": "introduced", "message": "RequestClass in the domain and docs section 10 class a declared outcome answered 501 or 503 differently, and ESS accepts such a binding"},
  {"file": "ess/domains/observe.yaml", "line": 523, "category": "acceptance", "severity": "note", "verdict": "CONFIRMED", "origin": "introduced", "message": "the graph and log rules (endpoints exist, unique ids, traffic iff not contains, reason/route vs class, requests <= capacity, totals = recorded) are fixture-only; the schema accepts violations, so the dependent reads-as-its-type tests miss them"},
  {"file": "ess/domains/observe.yaml", "line": 460, "category": "judgement", "severity": "note", "verdict": "INFEASIBLE", "origin": "introduced", "message": "binding node ids binding:<id> are not unique across specifications, though the topology is meant to span several behind one ingress"}
]
```
