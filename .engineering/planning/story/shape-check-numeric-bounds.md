---
format: aep.planning-md/3
id: story:shape-check-numeric-bounds
kind: story
status: draft
title: The UI's shape check refuses the numbers its schema refuses
tags:
- from-finding
relations:
- informed_by: review-result:w12-u4-adversary-1
- serves: vision:spindle
revision: 1
---
# The UI's shape check refuses the numbers its schema refuses

## Outcome

`ui/src/lib/conform.ts` checks a number member only with `typeof number` (line 68 at wave 12's
base), so it accepts negative counts, fractions and statuses outside 100-599 that the generated JSON
Schema refuses. It does so for the existing `Sample` answer and for the request log the wave 12
domain adds. The shape check should refuse what the schema refuses for integer and bounded members.

## Acceptance

- `the request log shape refuses counts its schema refuses` and `the sample shape refuses a
  sequence its schema refuses` (`ui/src/api/adversary_ingress.test.ts`) assert the refusal; they
  assert today's acceptance until this story lands.

## Origin

`review-result:w12-u4-adversary-1`, finding on `ui/src/lib/conform.ts:68`, origin pre-existing.
