---
format: aep.planning-md/3
id: story:links-second-hello-race
kind: story
status: draft
title: Two hellos in one burst are refused however they arrive
relations:
- serves: vision:spindle
- informed_by: story:links-hello-test-timing
revision: 1
---
# Two hellos in one burst are refused however they arrive

## Outcome

`adversary_links::a_second_hello_before_the_link_opens_is_refused`
(`crates/spindle-host/tests/spindle-host/adversary_links.rs:281`) failed again on macOS (CI run
42399676454, job 128484526924, PR 31, a store-only change): `refused` was empty after the 10 s
deadline `story:links-hello-test-timing` introduced, so the earlier diagnosis (a fixed wait too
short for a slow runner) does not hold.

The cause is in `Host::accept` (`crates/spindle-host/src/host/component.rs`, the loop over
`remote.pending`): a pending link is polled once per call. When that poll has read only the first
`hello`, the link has a peer and is linked; the second `hello` then arrives on an open link and
breaks it on the receiver's path, which `Host::refused` never reports. When both hellos are read
in one poll, the link is refused as the test expects. Which happens depends on how the two writes
are segmented, so the outcome is not decided by the protocol.

## Acceptance

- A connection that says `hello` twice before the link opens is refused, reported by
  `Host::refused`, whether the second `hello` arrives in the same read as the first or after it;
  a test drives both orders deterministically (one write holding both frames, and two writes with
  the link accepted in between).
- The existing test passes 20 times in a row on one host under `nice -n 19` with 4 test threads.

## Origin

Store-close PR for wave 14: the macOS lane went red on this test while the change touched only the
planning store.
