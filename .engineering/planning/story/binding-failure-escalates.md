---
format: aep.planning-md/3
id: story:binding-failure-escalates
kind: story
status: active
title: A binding whose on_failure policy escalates runs
relations:
- serves: vision:spindle
- informed_by: story:serve-what-runs
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-channel/src/route.rs
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/lines.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/adversary_unserved.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/escalation.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/stdin_bindings.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/unserved.rs
- confidence: cited
  path: crates/spindle-conformance/src/target.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/escalation.rs
- confidence: inferred
  path: crates/spindle-conformance/tests/spindle-conformance/main.rs
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/fixtures/escalation
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/retries.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: cited
  path: website/data/status.json
- confidence: cited
  path: website/docs/reference/cli.md
- confidence: cited
  path: website/docs/status.md
revision: 32
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T08:24:13Z", actor: "human:operator", revision: 31, decided_on: {"recorded":{"review_outcome":5}}}
- {from: "proposed", to: "active", at: "2026-11-27T08:24:13Z", actor: "human:operator", revision: 32, decided_on: {"recorded":{"review_outcome":5}}}
---
## Outcome

A binding whose `on_failure` policy escalates (`escalate: {emits: <event>, outcomes: [...]}`, beside
`drop:` and `retry:` entries per refusal, or the universal `on_failure: {escalate: ...}`) runs in
`run` (served and stdin) and `conform`: when the bound command is refused with one of the escalated
outcomes, the host emits the named event and invokes nothing, as ESS 0.99.1 defines it. Where the
command input lacks a field of the event, the host emits nothing and reports the failure as
`unmet`. Today `run` exits 1 before it listens: "unsupported: binding <id> says on_failure:
Escalate; this host drops or retries an invocation that fails and does not escalate it"
(`crates/spindle-host/src/host.rs:1012-1017`, `uncarried`).

## Why

Reported by a consumer on 2026-11-27 against 0.22.0: one binding of its specification carries a
per-refusal policy with an escalation, so `run` refuses the whole specification and nothing is
served. With the escalation removed from a scratch copy, the same run listens and serves.
`story:serve-what-runs` left `on_failure: escalate` refused.

## Acceptance

- `a_served_run_with_an_escalating_binding_listens`: a neutral fixture whose binding drops one
  refusal, escalates two and retries the rest; `run` listens and answers a command. It exits 1 on
  0.22.0.
- `an_escalated_refusal_emits_the_named_event_and_invokes_nothing`: the bound command refused with an
  escalated outcome publishes the named event once, with the payload ESS's interpreter
  (`ess_conformance::interpret`) gives where the command input holds every field of the event, and
  no further invocation of the bound command follows.
- `a_dropped_refusal_is_still_dropped`: a refusal listed under `drop:` is dropped as today.
- `a_universal_escalation_runs`: a binding with `on_failure: {escalate: ...}` escalates every
  refusal the same way.
- `a_stdin_line_escalates_a_refusal`: on a stdin run the refused command's line carries
  `"escalated": {"event": ..., "payload": ...}`.
- `an_escalation_event_field_the_input_lacks_is_reported_unmet`: no event is published and the
  failure is reported as `unmet`.
- `drop_retries_keeps_escalations`: with `--drop-retries` an escalated refusal still escalates.
- `conform_runs_an_escalating_binding`: `conform` on the fixture executes every scenario that selects an escalated refusal (none is skipped as `unsupported`; on 0.22.0 each is), and every executed scenario passes.
- `a_binding_escalating_to_an_event_the_component_does_not_publish_is_refused_at_start`: `run`
  exits 1 before it listens, naming the binding, the event and the component.
- `an_optional_escalation_event_field_the_input_lacks_is_left_out`: the event is published without
  the field, and the failure is not reported as `unmet`.
- `an_escalation_event_reaches_a_binding_subscribed_to_it`: a second binding that listens for the
  escalation event invokes its command once, as ESS's interpreter dispatches it.
- `the_docs_say_an_escalation_is_not_stored`: a test reads `docs/design/spindle.md` (beside the
  failure policy, `:435`) and the `run` reference in `website/docs/reference/cli.md`, and finds in
  each that an escalation event goes to the outbox, is not in the event history, and is lost if the
  process stops before it is delivered.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` (host refusal and attempt loop) and `crates/spindle-channel` (route compile and answer) — cited
- **Files:** `crates/spindle-host/src/host.rs:1012-1017` (`Host::assemble`, the refusal the story names) — cited
- **Files:** `crates/spindle-host/src/host.rs:343-359` (`uncarried`, `escalates`; narrowed or removed), `:170` (`Report`, gains the escalations), `:2087` `Host::invoke`, `:2107` `Host::attempt` (stops on an escalating answer and publishes the event on the receiving component's `sender`) — cited (tree)
- **Files:** `crates/spindle-channel/src/route.rs:384-385` and `:393-394` (`Route::compile` refuses both escalation forms), `:29` (`Route`, needs the escalation event), `:213` (`Router::dropping_retries` clears `on_refusal` whole and would turn an escalation into a drop), `:255` (`Route::answer` returns only the `Failure` word, not the event) — cited (tree)
- **Files:** `crates/spindle-cli/src/lines.rs:477` (`attempted`), `:342` (`fire`), `:431` (`publish`): the stdin path stops, prints and publishes the escalation — cited (tree)
- **Files:** `crates/spindle-conformance/src/target.rs:321-325` (adds a report's escalations to the observed events, so `ExpectPublicationCount` sees them) — cited (tree)
- **Symbols:** `uncarried`, `escalates`, `Host::assemble`, `Host::attempt`, `Report`, `Route`, `Route::answer`, `Route::compile`, `Router::dropping_retries`, `Sender::publish` (`endpoint.rs:122`, refuses an event the component does not publish), `attempted`, `Lines::fire` — cited
- **Parity reference:** ESS 0.99.1 (rev d7833ee) `crates/verify/ess-conformance/src/interpret/bindings.rs:224-230` (universal), `:273-281` (per refusal), `:420-446` (`escalate`: payload = placeholder values overwritten by input fields of the same name; then dispatched further) — cited
- **Fixture:** new neutral fixture `crates/spindle-host/tests/fixtures/escalation/` (one binding: drop one refusal, escalate two, retry the rest; every escalation-event field is a command-input field) — inferred, placed like `fixtures/refusal`
- **Tests added:** `crates/spindle-conformance/tests/spindle-conformance/escalation.rs` (`an_escalated_refusal_emits_the_named_event_and_invokes_nothing`, `a_dropped_refusal_is_still_dropped`, `a_universal_escalation_runs`, `an_escalation_event_field_the_input_lacks_is_reported_unmet`, `an_optional_escalation_event_field_the_input_lacks_is_left_out`, `an_escalation_event_reaches_a_binding_subscribed_to_it`, host against `Interpreted` as `refusal.rs` does) — inferred
- **Tests added:** `crates/spindle-cli/tests/spindle-cli/escalation.rs` (`a_served_run_with_an_escalating_binding_listens`, `a_stdin_line_escalates_a_refusal`, `drop_retries_keeps_escalations`, `a_binding_escalating_to_an_event_the_component_does_not_publish_is_refused_at_start`, `conform_runs_an_escalating_binding`, `the_docs_say_an_escalation_is_not_stored`) — inferred; each expected well under 60 s and unmeasured, step budget 120 s
- **Tests changed:** `crates/spindle-cli/tests/spindle-cli/stdin_bindings.rs:190` (`a_binding_the_router_refuses_stops_the_run_before_stdin` uses `escalate.yaml`; point it at a construct still refused, such as a bounded retry), `crates/spindle-cli/tests/spindle-cli/unserved.rs:495`, `crates/spindle-cli/tests/spindle-cli/adversary_unserved.rs:416`, `crates/spindle-host/tests/spindle-host/retries.rs:62` — cited (tree), under D1-A
- **Test registries:** `crates/spindle-cli/tests/spindle-cli/main.rs`, `crates/spindle-conformance/tests/spindle-conformance/main.rs` — inferred
- **Documents:** `crates/spindle-cli/src/cli.rs:108,221` (doc comments say escalate exits 1), `website/docs/reference/cli.md:99,119`, `docs/design/spindle.md:435`, `crates/spindle-docs/src/status.rs:170-183` with generated `website/data/status.json` and `website/docs/status.md`, `CHANGELOG.md` — cited (tree)
- **Confidence:** high for the refusal sites and the attempt loops (the story and the tree name them); medium for where the event is published and recorded, which hangs on D2 and D3
- **Would collide with:** `crates/spindle-host/src/host.rs` (wave 28 entity-event-history, events-record-their-caller); `crates/spindle-conformance/src/target.rs` (wave 28 replay-with-decisions); `crates/spindle-cli/src/cli.rs`, `website/docs/reference/cli.md`, `CHANGELOG.md`, `crates/spindle-docs/src/status.rs` + generated status pages, `crates/spindle-cli/tests/spindle-cli/main.rs` (several wave 28 units); wave 29 ess-pin-0-99-2 if ESS 0.99.2 changes `ResolvedRefusalAction`, `ResolvedBinding` or the interpreter's `escalate`
- **Safety fact:** only two places act on a failure answer, `Host::attempt` (`host.rs:2135`) and stdin `attempted` (`lines.rs:498`). Both compare `Route::answer(..) == Failure::Retry`, so a new escalate branch beside it changes no drop or retry path. The other users of the failure policy (`spindle-conformance/src/target.rs:575`, `spindle-telemetry/src/catalogue.rs:220`) treat every word but retry as `false` — step 2 (`git grep '\.answer('`, `Failure::Retry`), unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: both the universal `on_failure: {escalate: ...}` and the per-refusal `escalate:`
  run; they share one path, and the four tests that pinned the refusal change with them.
- D2 A: the component whose command was refused publishes the event through its sender
  after the attempt; a binding whose receiving component does not declare the event in
  `publishes:` is refused at start, naming both.
- D3 A: the event goes to the outbox, not the store's event log, so it is not in the
  event history and is lost if the process stops before delivery; the docs say so. Recording it in
  the store would change the stored format and waits for the operator.
- D4 A: an escalation event field the command input does not hold publishes no event and
  reports the failure as `unmet` at run time; an optional field is left out. ESS's interpreter
  fills a placeholder there, which ESS's design note forbids a runtime to do.
- D5 A: on stdin the refused command's line gains
  `"escalated": {"event": ..., "payload": ...}`.
- D6 A: `--drop-retries` turns retry entries into drop and keeps escalate entries.
- D7 A: the escalation event is dispatched to the bindings that listen for it like any
  event the component publishes, as ESS's interpreter dispatches it.
