---
format: aep.planning-md/3
id: story:links-hello-test-timing
kind: story
status: implemented
title: The second-hello link test does not depend on how fast the runner is
tags:
- from-finding
relations:
- informed_by: approval-record:release-0.4.0-macos-red
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T04:21:46Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-07T04:21:46Z", actor: "human:timo", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-07T06:21:24Z", actor: "human:timo", revision: 4, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# The second-hello link test does not depend on how fast the runner is

## Outcome

`adversary_links::a_second_hello_before_the_link_opens_is_refused`
(`crates/spindle-host/tests/spindle-host/adversary_links.rs:275`) gives the table a fixed 300 ms
(`accept_for_a_while`: 60 accepts with a 5 ms sleep, lines 145-153) to read two hellos from one
connection. On main's CI run 42326647238 for 2c211a9 the macOS job failed this assertion (83
passed, 1 failed); the same job passed on the PR head 058292b and on the rerun of that run. The
test should wait for the condition it asserts (a refusal recorded, or the connection closed),
bounded by a generous deadline, instead of a fixed number of steps.

## Acceptance

- The test polls until the table has refused the connection or a deadline of several seconds
  passes, and asserts the same outcome as today.
- Run 20 times in a row on one host under `nice -n 19` with 4 test threads: 20 passes.

## Origin

Release check of 0.4.0 (`approval-record:release-0.4.0-macos-red`): the release run's gate on the
tagged commit failed on this test, and a rerun passed.
