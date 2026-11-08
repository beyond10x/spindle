---
format: aep.planning-md/3
id: story:links-second-hello-race
kind: story
status: implemented
title: Two hellos in one burst are refused however they arrive
relations:
- serves: vision:spindle
- informed_by: story:links-hello-test-timing
scope:
- confidence: cited
  path: crates/spindle-host/src/host/component.rs
- confidence: cited
  path: crates/spindle-host/tests/spindle-host/adversary_links.rs
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-07T18:59:09Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-07T18:59:09Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-08T14:30:57Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1,"verification":1}}}
---
# Two hellos in one burst are refused however they arrive

## Outcome

`adversary_links::a_second_hello_before_the_link_opens_is_refused`
(`crates/spindle-host/tests/spindle-host/adversary_links.rs:281`) failed again on macOS (CI run
42399676454, job 128484526924, PR 31, a store-only change). `refused` was still empty after the
10 s deadline that `story:links-hello-test-timing` introduced, so the earlier diagnosis (a fixed
wait too short for a slow runner) does not hold.

`Host::accept` (`crates/spindle-host/src/host/component.rs`, the loop over `remote.pending` at
line 601) reads what a pending link has buffered before it links. `Link::poll` (line 156) reads
until the socket would block and takes every whole message. A second `hello` read in the same poll
violates the link, and `Host::refused` (line 648) reports it. A second `hello` that arrives after
the poll that linked arrives on an open link, and `Host::deliver` answers it as `HostError::Peer`
naming the peer. The test at line 281 writes the two hellos in two writes with no accept call
between them. Which of these two cases it hits depends on whether one accept's poll falls between
the writes, and that is decided by the runner, not by the protocol.

The two arrivals become two deterministic tests, and each states its transition:

- **A, one write.** Both hellos arrive in one write before any accept. The pending link moves to
  refused: `Host::refused` is non-empty, and the peer named by the second hello is back in
  `Host::unlinked`, as the assertion at line 288 already states.
- **B, two writes with an accept between.** The first hello is written, and `Host::accept` is
  called until the link opens, which takes longer than one accept. Then the second hello is written.
  The open link moves to broken: `Host::deliver` answers `HostError::Peer` naming the first hello's
  peer. This is the open-link case that
  `a_second_hello_on_an_open_link_is_refused_naming_the_peer` (line 295) asserts. That test stays
  unchanged and passing, and B is it.

On current main neither A nor B fails. The test at line 281 is what fails, intermittently: it
expects A's result while its two writes can land in B's case. This story changes no host code. It
replaces the two writes of the test at line 281 with one write holding both frames, which is A.

## Acceptance

- A: `a_second_hello_before_the_link_opens_is_refused` keeps its name and sends both `hello` frames
  in one `write_all`. It asserts that `Host::refused` is non-empty and that `Host::unlinked`
  contains the second hello's peer as `(from)`.
- B: `a_second_hello_on_an_open_link_is_refused_naming_the_peer` is unchanged and passes. After
  `Host::accept` opened the link, the second `hello` makes `Host::deliver` answer
  `HostError::Peer` naming the first hello's peer.
- A secondary note, not the acceptance: `cargo test -p spindle-host --test spindle-host
  adversary_links::` passes 20 times in a row on one host under `nice -n 19` with 4 test threads.

## Scope

- `crates/spindle-host/src/host/component.rs`: cited. `Host::accept`'s loop over `remote.pending`
  (line 601), `Link::poll` (line 156) and `Host::refused` (line 648). Read to state the
  transitions, not changed.
- `crates/spindle-host/tests/spindle-host/adversary_links.rs`: cited.
  `a_second_hello_before_the_link_opens_is_refused` (line 281) and
  `a_second_hello_on_an_open_link_is_refused_naming_the_peer` (line 295).

No new test module is added, so no `mod` line changes.

## Order

No active or draft story records a scope on either file.

## Origin

The store-close PR for wave 14: the macOS lane went red on this test while the change touched only
the planning store.
