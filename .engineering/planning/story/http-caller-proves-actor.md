---
format: aep.planning-md/3
id: story:http-caller-proves-actor
kind: story
status: active
title: An HTTP request proves its caller, and a command that reads the caller runs for it
tags:
- downstream-gap
relations:
- serves: vision:spindle
- informed_by: story:unauthenticated-grant-refusals
scope:
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/src/router.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: inferred
  path: ess/components.yaml
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 7
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:34Z", actor: "human:timo", revision: 3}
---
# An HTTP request proves its caller, and a command that reads the caller runs for it

## Outcome

A run serving HTTP turns a proof carried by the request into the actor and the caller attributes
that commands read (`{caller: <attribute>}`, `caller.<attribute>` in guards). A command that reads
its caller then runs for that caller instead of answering 501, and a request without a valid proof
is refused. How the proof is carried (for example a header set by a trusted proxy in front of the
ingress) is a design decision recorded in `docs/design/spindle.md` §11, which today says
"undecided" (`docs/design/spindle.md:402`).

Today: `crates/spindle-ingress/src/surface.rs:1088` answers 501 "the caller attributes it reads
(…), which no HTTP caller proves here" unless `--fixed-actor`/`--fixed-caller` fixes one caller for
the run, a test switch. ESS 0.90 leaves the proof to the realization (`ess-gen` `src/http.rs:97-100`
at 0.90.0, cited by wave 17 unit U8).

Related: `story:unauthenticated-grant-refusals` refuses a request with no caller as ESS does; this
story supplies the caller. The downstream need is a deployment that runs without `--fixed-actor`.

## Acceptance

On a neutral fixture with a command whose outcome payload carries `{caller: person_id}`, `run`
serving HTTP without `--fixed-actor`/`--fixed-caller` commits that outcome with the payload's
`person_id` equal to the one the request proves, and answers a request carrying no proof with the
refusal the design names.

## Fails closed

A request without the proof is refused and never mapped to a default actor. The trusted source of
the proof is configured explicitly; with nothing configured, a command that reads its caller is
refused as today. (Required by the accepting coordinator, `approval-record:downstream-gaps-acceptance`.)

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` — cited
- **Files:** `crates/spindle-ingress/src/surface.rs:1083-1094` (the 501 "which no HTTP caller proves here" gate, `route.caller.is_none()`), `:103-117` (`Caller`, `Route::caller`) — cited
- **Files:** `crates/spindle-ingress/src/answer.rs:91,225-230` (command sent as `route.caller()` into `host.submit_refusing`) — cited, via `story:unauthenticated-grant-refusals` and the tree
- **Symbols:** `spindle_ingress::Caller`, `Route::caller`, `Surface::fixing_caller`, `Surface::fixing_callers`, `spindle_kernel::caller_reads` — cited
- **Also likely:** `crates/spindle-ingress/src/router.rs:22-33` (`Request` carries no headers today; a proof carried by the request has to enter here) — inferred
- **Also likely:** `crates/spindle-ingress/src/listener.rs` (hyper request → `Request`; already reads `AUTHORIZATION`/`HOST` for the console guard, `:125-164`) — inferred
- **Also likely:** `crates/spindle-cli/src/cli.rs:186-202`, `crates/spindle-cli/src/serve.rs:91-151,221-240`, `crates/spindle-cli/src/main.rs:186-200` (configuring the trusted proof source next to `--fixed-actor`/`--fixed-caller`) — inferred
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/` plus a new fixture under `crates/spindle-cli/tests/fixtures/` — inferred
- **Documents:** `docs/design/spindle.md` §11, the "undecided" row at `:422` (the story cites `:402`, which has drifted) and the fixed-caller row at `:407` — cited
- **Documents:** `website/docs/reference/cli.md` (generated from clap), `website/docs/status.md:53,99` — inferred
- **Documents:** `ess/components.yaml:37-43` (`UNMAPPED:` "how a caller proves which actor it is … Open question for the owner") — inferred, revisited once the design decides
- **Confidence:** medium — the defect site and the 501 gate are cited; where the proof enters and how it is configured depend on a §11 decision not yet made
- **Would collide with:** units touching `spindle-ingress`'s route/caller surface (`surface.rs`, `answer.rs`, `router.rs::Request`), `spindle-cli`'s `run` HTTP flags (`cli.rs`/`serve.rs`), or design §11; notably `story:unauthenticated-grant-refusals` (same `answer.rs` caller path)
- **Safety fact:** today no request content reaches the caller: `router::Request` carries no headers (`router.rs:22-33`), and the kernel gets a caller only from the route fixed at start (`answer.rs:225-230`). Keeping the 501 at `surface.rs:1086` when no trusted source is configured keeps it fail-closed — unproven
- **Not established:** wave 17 did not specify the proof: `ess/components.yaml:37-43` holds it as `UNMAPPED:` and §11 says "undecided". The status and body of a request with no proof (401, or ESS's 403 with `actor: null`) are undecided. Whether the trusted source is a CLI flag, the environment or an overlay is undecided.
