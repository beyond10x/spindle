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
revision: 4
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
