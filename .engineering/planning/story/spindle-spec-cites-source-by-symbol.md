---
format: aep.planning-md/3
id: story:spindle-spec-cites-source-by-symbol
kind: story
status: draft
title: The specification cites its sources by symbol, and a check holds them
revision: 1
---
# The specification cites the source by symbol, and the citations are checked

## Outcome

Every source citation in `ess/domains/*.yaml` names a file and a symbol (a function, a type, a
constant) or a README section, never a line range, and a check fails when a cited file or symbol
does not exist. Today `ess/domains/console.yaml` and `ess/domains/observe.yaml` cite about forty
line ranges, and the ones a design review checked had moved: they point at other code.

## Source

ESS hardening round 2026-11-23 (`docs/hardening/2026-11-23.md`, technique 8, findings 16 to 20).
Examples on `bbc50b00`: `console.rs:109-139` is `GET /api/events`, the reset handler is elsewhere
in the file; `router.rs:16` is not the body limit; `host.rs:855-872` is store opening, not
`reset_to`; `README.md:61` is about answer sources, the reset sentence is in another section.
The round rewrote the citations of `ess/domains/deployment.yaml` and
`ess/domains/environment.yaml` by symbol; `console.yaml` and `observe.yaml` keep theirs.

## Acceptance

- `every_cited_symbol_exists`: a test in `crates/spindle-spec` reads every citation of
  `ess/domains/*.yaml` and finds each cited file, and each cited symbol in it; a citation with a
  line number fails it.
- `ess/domains/console.yaml` and `ess/domains/observe.yaml` cite by symbol; `ess specify validate`
  and the observe drift check pass after `task generate:observe`.

## Scope

- `ess/domains/console.yaml`, `ess/domains/observe.yaml`, the generated observe projections.
- `crates/spindle-spec/tests/`.
