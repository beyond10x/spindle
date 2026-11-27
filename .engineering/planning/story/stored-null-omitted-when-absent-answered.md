---
format: aep.planning-md/3
id: story:stored-null-omitted-when-absent-answered
kind: story
status: draft
title: A stored null in an omitted_when_absent view field is answered null
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A view field declared `presence: omitted_when_absent` whose row holds a stored `null` (written by
a scenario's establish step) is answered by the conformance target as ESS's interpreter answers
it: `null`. Today `Host::listed_with` drops the stored `null` before the target sees the row, so
the field is left out.

## Why

Found by wave 30's U2 implementor (head ccf4e518) while making unwritten optional view fields
answer as ESS. The cause is in the host's projection of a stored row, not in the view rows the
story changed.

## Acceptance

- `a_stored_null_in_an_omitted_when_absent_field_is_answered_null`: a neutral fixture whose
  establish step writes `null` into an `Optional` field declared `omitted_when_absent`; the
  conformance target's `query_view` answers `null` for it, as `ess_conformance::interpret` does.
