---
format: aep.planning-md/3
id: story:link-bound-setting
kind: story
status: draft
title: The peer link bound is a setting, so its Gate test runs short
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

The bound within which a peer link must come up (`LINK_WITHIN`, 30 s today) is a setting with the
30 s default, so the Gate's test of a peer that never listens runs with a short bound and ends
within 20 s.

## Why

The test-time budget work of 2026-11-28 (`integration/2026-11-28-1`) left one Gate test over 20 s:
`a_peer_that_never_listens…` waits the product's `LINK_WITHIN` of 30 s, so it cannot drop below
that without a production change. The Gate nextest step ran 84.5 s at best on a 4-CPU slice against
a target of 80 s.

## Acceptance

- The link bound is read from a setting whose default is 30 s; `run --help` or the reference page
  that lists the other bounds names it.
- The peer-never-listens test sets the bound to at most 3 s and ends within 10 s.
- A test shows the default is 30 s when the setting is absent.
