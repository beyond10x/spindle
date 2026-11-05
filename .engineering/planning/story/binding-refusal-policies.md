---
format: aep.planning-md/3
id: story:binding-refusal-policies
kind: story
status: draft
title: Binding policies per refusal
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
- depends_on: story:event-bindings-retry-on-failure
scope:
- confidence: cited
  path: crates/spindle-channel/src/route.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/retry.rs
revision: 4
---
# Binding policies per refusal

## Outcome

The host runs an event binding whose failure policy is chosen per refusal (`on_failure:` written
as `{drop: [<outcome>…], retry: {except: [<outcome>…]}}`, compiled to `on_refusal`): an attempt
refused with a listed outcome is answered by that outcome's policy (drop ends the occurrence at
once), and any other refusal by the fallback. Today `crates/spindle-channel/src/route.rs` refuses
every such binding ("an `on_refusal` policy").

Requested by a downstream specification probe (2026-11-05, 16 bindings): 5 bindings drop on named
refusals and retry the rest. `story:event-bindings-retry-on-failure` covers the plain
`on_failure: retry` of the other 11; this story covers only the selection per refusal.
Reproduced with a neutral fixture.

ESS semantics: ESS 0.86.0's interpreter, `crates/verify/ess-conformance/src/interpret/bindings.rs`
(`refusal`, `selection`, `Interpreted::selected`).

## Acceptance

Fixture: a library binding `LoanClosed -> SendThanks` with `on_failure: {drop: [no-reader],
retry: {except: [no-reader]}}`.

- `a_listed_refusal_is_dropped_after_one_attempt` — `no-reader` ends the occurrence; one attempt
  is counted.
- `another_refusal_falls_back_to_retry` — a target refusing twice with another outcome and then
  accepting is attempted three times.
- `refusal_policies_agree_with_the_interpreter` — for the same scenario, attempts and resulting
  rows equal what ESS 0.86.0's interpreter produces.

## Scope

- `crates/spindle-channel/src/route.rs` — cited: the `on_refusal` refusal in `Route::compile`.
- `crates/spindle-host/src/host.rs` — cited: the failure policy check in `Host::new`.
- `crates/spindle-host/tests/retry.rs` — inferred, created by
  `story:event-bindings-retry-on-failure`.

## Depends on

- `story:event-bindings-retry-on-failure` — the retry path the fallback uses.
