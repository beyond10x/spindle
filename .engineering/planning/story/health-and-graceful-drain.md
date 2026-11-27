---
format: aep.planning-md/3
id: story:health-and-graceful-drain
kind: story
status: draft
title: A served run answers liveness and readiness, and drains on SIGTERM
summary: Liveness and readiness routes, readiness after the seed, and SIGTERM answering the requests in flight before exit.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- depends_on: story:seed-readiness
- informed_by: story:serve-loop-signal-exit-answers
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed/health.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/seed/signal_exit.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/close.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/health.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: crates/spindle-observe-api
- confidence: inferred
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/generated/jsonschema
- confidence: inferred
  path: ui/src/api/generated
- confidence: inferred
  path: website/docs/concepts/observe.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 26
---
# A served run answers liveness and readiness, and drains on SIGTERM

## Outcome

A served `run` answers a liveness route while its host thread answers, and a readiness route
`200` only once its stores are open and its seed has ended (`story:seed-readiness`). On SIGTERM it
turns readiness to `503`, stops accepting, answers the requests already received, and then exits
143.

## Why

- No health route exists in any crate (no `/health`, `/livez` or `/readyz` route under `crates/`).
- On a signal the serve loop returns at once (`crates/spindle-cli/src/serve.rs:442-447`), and
  dropping the listener answers every request not yet answered `503`
  (`crates/spindle-ingress/src/listener.rs:215`, `listener.rs:368-375`). A rolling restart of a
  consumer's service fails the requests in flight.
- `story:seed-readiness` covers whether the seed has ended; it adds no liveness and no drain.

## Acceptance

- `liveness_answers_200_while_the_host_answers` (ingress): `GET /__spindle/live` answers `200`
  while the host thread calls `Listener::next` within the bound the test passes, and `503` once it
  stops.
- `the_liveness_route_needs_no_observe`: `/__spindle/live` answers without `--observe`; on a
  `--trusted-caller-header` run it answers `403` without the proxy secret. (`/__spindle/ready` is
  covered by `story:seed-readiness`.)
- `readiness_reads_503_once_sigterm_arrives`: a served run whose seed ended reads `200` on
  `/__spindle/ready`, then `503` as soon as SIGTERM arrives, while a command is still held.
- `sigterm_answers_the_requests_in_flight`: one command held by a slow external cause and one queued
  when SIGTERM arrives are both answered with their outcomes, not `503`; a request sent after the
  signal is answered `503` at once; the run exits 143 within the `--drain` bound.
- `a_request_queued_past_the_drain_bound_reads_503`: with `--drain 1`, a request still queued when
  the bound passes is answered `503` and the run exits 143.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `GET /__spindle/live` beside seed-readiness's `GET /__spindle/ready`.
- D2 B, not the scoper's A: probes present the proxy secret on a
  `--trusted-caller-header` run, as every `/__spindle/` route; no guard is loosened. An orchestrator
  probe sends the header (Kubernetes `httpGet.httpHeaders`). Taken by the coordinator so that no
  guard changes in this wave.
- D3 A: liveness answers 200 while the host thread called `Listener::next` within a
  bound above the 30 s external-call cap, 503 after; tests pass a short bound at the ingress level.
- D4 A: readiness turns 503 when the signal flag is set; a request arriving after it is
  answered 503 at once; the loop serves the requests queued at the signal up to the drain bound,
  then closes. This replaces D1 A of `story:serve-loop-signal-exit-answers` (taken on default in
  wave 27) and rewrites its test `a_request_queued_at_the_signal_reads_503`.
- D5 A: `run --drain <seconds>`, default 20; the request in flight always finishes.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` listener (health routes, drain) and `crates/spindle-cli` serve loop — cited
- **Files:** `crates/spindle-ingress/src/listener.rs` — `Listener` `:219-224`, `Stop` `:228-234`, server accept loop `:352-379`, `Listener::next` `:396-398`, `Listener::close` `:406-418`, secret checks in `answer` `:503-508` (a probe off loopback is refused 403 there before any route is read), reserved dispatch `:509-544` — cited
- **Files:** `crates/spindle-ingress/src/observe.rs` — `Reserved` `:82-113` (new variants for liveness and readiness), `reserved` `:1227-1263` (every `/__spindle/` path answers 404 without `--observe`, `:1236-1238`) — cited
- **Files:** `crates/spindle-cli/src/serve.rs` — loop `:466-539`, `signal_exit` `:457-463`, exchange taken with the signal answered 503 `:491-494`, `listener.close(CLOSE_GRACE)` `:542`, `CLOSE_GRACE` `:547`, doc `:265-268` — cited
- **Files:** `crates/spindle-cli/src/main.rs` — `signals` `:316-324` (signal_hook flag, set without the host thread, so readiness can read it at once), `SIGNAL_POLL` `:438`, `serve::Serving` construction `:604` — cited
- **Files:** `crates/spindle-cli/src/cli.rs:136-142` — `run` help on SIGINT/SIGTERM ("answers each request still waiting 503"), wrong under D4 A; a drain flag under D5 A — cited
- **Symbols:** new liveness heartbeat on `Listener` (instant of the last `next`), a shared readiness/draining state read on the listener thread, a drain step before `Listener::close` — inferred
- **Tests:** new `crates/spindle-ingress/tests/spindle-ingress/health.rs` + `mod health;` in `main.rs`: `liveness_answers_200_while_the_host_answers` (short bound passed to the listener; stop calling `next` reads 503), `the_liveness_route_needs_no_observe` (`/__spindle/live` only; the ready route is `story:seed-readiness`'s), `readiness_reads_503_once_sigterm_arrives` — inferred
- **Tests:** new `crates/spindle-cli/tests/spindle-cli/seed/health.rs` + `mod health;` at `seed.rs:1225-1230`, using `holding_desk` (`seed.rs:971`) and `serve`: `sigterm_answers_the_requests_in_flight` (one command held, one queued, SIGTERM: both answered with their outcomes under D4 A, a request after the signal refused, exit 143 within the bound), `readiness_reads_503_once_sigterm_arrives` — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/seed/signal_exit.rs:141-187` `a_request_queued_at_the_signal_reads_503` rewritten under D4 A — cited
- **Also likely:** `crates/spindle-ingress/src/surface.rs` (where seed-readiness puts its readiness state), `crates/spindle-ingress/tests/spindle-ingress/close.rs` — inferred
- **Documents:** `website/docs/reference/cli.md` (generated from `cli.rs`), `website/docs/concepts/observe.md`, `AGENTS.md` map rows of `spindle-ingress` and `spindle-cli` (both describe `Listener::close`), `ess/domains/observe.yaml` with `ess/generated/jsonschema`, `crates/spindle-observe-api`, `ui/src/api/generated` under D1/ESS A — inferred
- **Confidence:** high for `serve.rs`/`listener.rs`/`observe.rs` (story names the drain site, tree read); medium for the health mechanism (depends on how seed-readiness lands its readiness route)
- **Would collide with:** any unit touching `spindle-ingress`'s `listener.rs` (`Listener`, `answer`, accept loop) or `observe.rs` (`Reserved`, `reserved`); `spindle-cli`'s `serve.rs` loop and `Serving`, `main.rs` `Serving` construction, `run` help in `cli.rs` and generated `cli.md`; the `mod` list of `tests/spindle-cli/seed.rs`; `ess/domains/observe.yaml` and its generated outputs
- **Safety fact:** `Listener` is also used by `crates/spindle-cli/src/component.rs` and `crates/spindle-control/src/bin/spindle-control-fixture.rs`, which rely on `Drop` only; a heartbeat and a drain step that only `serve` calls change neither — step 2 (the `Listener::bind` callers named in story:serve-loop-signal-exit-answers' scope), unproven
