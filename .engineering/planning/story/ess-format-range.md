---
format: aep.planning-md/3
id: story:ess-format-range
kind: story
status: active
title: A specification in any supported ESS format runs unchanged
relations:
- serves: vision:spindle
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T17:56:29Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-10T17:56:29Z", actor: "human:timo", revision: 3}
---
# A specification in any supported ESS format runs unchanged

## Outcome

Spindle reads a specification whose `format:` is `ess/34`, `ess/35` or `ess/36` without anyone
editing it, and reads a newer format the same way as far as its constructs are known: a construct
Spindle does not implement is refused by name, never the whole specification.

## Why

Reported by a downstream consumer on 2026-11-10 as the operator's requirement. Spindle 0.9.0 is
built on ESS 0.90.0, which reads formats up to `ess/35`. A specification compiled with ESS 0.92.0
declares `ess/36` and is refused whole: 0 of 43 commands run, `admit` refuses it, the HTTP host does
not start. The same specification with only its format line set to `ess/34` runs 41 of 43 commands,
serves 39 of 43 commands and 11 of 11 views, and replays 18 of 28 scenarios. It uses no `ess/35`
or `ess/36` construct, so the refusal is the format version alone.

## Acceptance

- One fixture per supported format (`ess/34`, `ess/35`, `ess/36`), identical but for the format
  line, each `admit`s, serves its commands over HTTP and passes the same conformance counts.
- A specification declaring a format newer than the newest Spindle's ESS implements, using only
  known constructs, runs; one using an unknown construct is refused naming that construct.
- `conformance.toml` or a regression test holds the per-format fixtures, so dropping a format
  fails the gate.

## Open

- Whether the ESS library reads a newer format leniently today, or Spindle must pin ESS 0.92.0
  (the newest release, which implements `ess/36`) and ask ESS for a forward-reading mode: not
  established.
