---
format: aep.planning-md/3
id: review-result:w12-u1-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on outcome-scoped-refusal (wave 12)
relations:
- reviews: story:outcome-scoped-refusal
revision: 1
---
Adversary pass 1 on `story:outcome-scoped-refusal`, wave 12 unit u1, change ed97240; the
adversary's 5 green cases are committed as 3b5a2f0 (module `adversary_outcome_scoped_refusal`).
Recorded from the adversary's report; absolute scratch paths are left out (the repository is public).

```
verdict: CONFIRMED
cases: executed 189→197, red 3
origin: introduced 1 / pre-existing 2 / undecided 0
```

Green, committed: a refused request draws nothing from the seeded mint; a sealed crate answers
wrong-state whichever branch the request selects; an actor not granted is refused before the
unsupported outcome; a bound wrong-state answer the ingress does not write refuses where selected;
an outcome a binding selects is refused without writing half (under `on_failure: retry`).

Red, uncommitted at report time (`adversary_outcome_scoped_refusal_red.rs`):

```
admit_and_run_agree_on_a_command_none_of_whose_outcomes_runs
  admit says a request selecting yard.crate.WeighCrate/weighed is refused and the command's other
  outcomes run, while run refuses every request of yard.crate.WeighCrate as not served
run_reading_stdin_names_each_unsupported_outcome_at_startup
  run reading stdin names no unsupported outcome at startup
a_compensating_refusal_refuses_only_the_request_that_selects_it
  "unsupported: a refusal that changes its row (`compensates: true`), in `shop.order.JoinOrder/failed`"
  left: Null, right: "joined"
```

Attacked and not broken: kernel order (grant, selects, input, selection, runs before any mint or
write) on every path; the seeded mint; telemetry counting a refusal as `failed.unsupported`; the
wrong-state answer and the 403 before the 501; the binding path with retries; `conform` and
`run --scenario` on the unit's fixture; `admit --json` stdout unchanged.

```findings
[{"file": "crates/spindle-cli/src/main.rs", "line": 630, "category": "contract-drift", "severity": "warning", "verdict": "CONFIRMED", "origin": "introduced", "message": "admit lists the outcome of a command none of whose outcomes runs as refused only where a request selects it, while run marks the command not served and the kernel refuses every request of it, invalid input answering 501 instead of 400."}, {"file": "crates/spindle-kernel/src/supported.rs", "line": 233, "category": "acceptance", "severity": "warning", "verdict": "CONFIRMED", "origin": "pre-existing", "message": "compensates (and by the same classification affects: and sets: without a subject) is refused as a construct selection reads although neither the kernel's select nor ESS's PrecedencePlan reads it, so a request selecting the command's supported outcome is refused whole."}, {"file": "crates/spindle-cli/src/main.rs", "line": 534, "category": "acceptance", "severity": "note", "verdict": "CONFIRMED", "origin": "pre-existing", "message": "run reading stdin prints no line naming an unsupported outcome at startup, which the acceptance asks of run and only the HTTP run does."}]
```

Coordinator's routing: all three go back to the implementor. Finding 1 is introduced. Findings 2
and 3 are pre-existing, but the story's outcome ("a command with one outcome spindle cannot run
stays served") and its acceptance ("`admit` and `run` startup name the outcome") do not hold
without them, and both fixes sit in the files the unit already changed.
