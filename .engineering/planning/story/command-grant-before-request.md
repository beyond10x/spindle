---
format: aep.planning-md/3
id: story:command-grant-before-request
kind: story
status: implemented
title: A command grant is checked before its request is read
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-11T09:40:08Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-11T09:40:08Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-11T20:31:02Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
# A command grant is checked before its request is read

## Outcome

For a command whose caller is not granted, spindle answers 403 before it reads the body, the
query string or the path, as the ESS 0.87.0 served application does
(`ess-synth/src/rust/http/bound.rs:712-718`). Today the grant is checked in the kernel after the
input is parsed, so an ungranted caller with a malformed body gets 400.

## Acceptance

- An ungranted fixed caller sending a malformed JSON, form or query request to a command gets 403.
- A granted caller sending the same request still gets 400.
- Transport limits (411, 413, 408) stay before the grant check.

## Origin

Found while fixing the adversary findings on `story:ingress-view-parameters` (2026-11-06); views
were fixed there, commands were not.
