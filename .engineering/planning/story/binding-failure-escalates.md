---
format: aep.planning-md/3
id: story:binding-failure-escalates
kind: story
status: draft
title: A binding whose on_failure policy escalates runs
relations:
- serves: vision:spindle
- informed_by: story:serve-what-runs
revision: 1
---
## Outcome

A binding whose `on_failure` policy escalates (`escalate: {emits: <event>, outcomes: [...]}`, beside
`drop:` and `retry:` entries per refusal) runs in `run` (served and stdin) and `conform`: when the
bound command is refused with one of the escalated outcomes, the host emits the named event and
invokes nothing, as ESS 0.99.1 defines it. Today `run` exits 1 before it listens:
"unsupported: binding <id> says on_failure: Escalate; this host drops or retries an invocation that
fails and does not escalate it" (`crates/spindle-host/src/host.rs:1012-1017`, `uncarried`).

## Why

Reported by a consumer on 2026-11-27 against 0.22.0: one binding of its specification carries a
per-refusal policy with an escalation, so `run` refuses the whole specification and nothing is
served. With the escalation removed from a scratch copy, the same run listens and serves.
`story:serve-what-runs` left `on_failure: escalate` refused.

## Acceptance

- `a_served_run_with_an_escalating_binding_listens`: a neutral fixture whose binding drops one
  refusal, escalates two and retries the rest; `run` listens and answers a command.
- `an_escalated_refusal_emits_the_named_event_and_invokes_nothing`: the bound command refused with an
  escalated outcome publishes the named event once, with the payload ESS's interpreter
  (`ess_conformance::interpret`) gives, and no further invocation of the bound command follows.
- `a_dropped_refusal_is_still_dropped`: a refusal listed under `drop:` is dropped as today.
- `conform_runs_an_escalating_binding`: `conform` on the same fixture; the escalation's scenario
  executes and passes.
