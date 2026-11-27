---
format: aep.planning-md/3
id: story:request-id-idempotency
kind: story
status: draft
title: A request id seen before is answered without a second write
summary: A request id seen before, across restarts and replicas, is answered without a second write.
relations:
- serves: vision:spindle
- informed_by: story:seed-at-start
revision: 2
---
# A request id seen before is answered without a second write

## Outcome

A request id seen before, across restarts and replicas, is answered without a second write: a
command sent again with a request id Spindle already answered gets the first answer back, and no
row, event or counter moves. This holds for a request through the ingress and for a seed entry
alike.

## Why

`story:seed-at-start` is `implemented` with two acceptance lines unmet: "each with the file's
actor, caller and request id" and "a repeated request id is deduplicated as any request is".
Spindle has no request-id idempotency for any request. `Host::submit_as`
(`crates/spindle-host/src/host.rs`) takes no request id, and the seed
(`crates/spindle-cli/src/seed.rs`, `Seeding::step`) only skips, within one start and in memory, an
entry whose id an earlier applied or already-there entry of the same file carried. So a restart
submits every entry again: an update entry writes over every change made since, a move entry is
refused, and two replicas starting at once each write. Only an `existing_instance:` create is safe
today. Found by two reviews of `0.19.0..0.19.1`.

## Acceptance

- `a_request_id_answered_before_is_not_written_twice`: a command sent twice with one request id,
  through the ingress, writes once; the second answer equals the first.
- `a_seed_restart_writes_no_update_twice`: a seed with an update entry, started twice on one state
  with a change between the starts, keeps the change; the second start counts the entry already
  there.
- `two_replicas_write_a_request_once`: two hosts on one PostgreSQL state, each submitting one
  request id, write it once.
- `a_request_id_with_another_command_is_refused`: a request id seen with another command, input,
  actor or caller is refused by name, not answered with the first answer.
- `a_retried_request_lands_once`: a request id sent to two `run` processes on one PostgreSQL store
  writes once. Moved from `story:several-writers-one-postgres` (its D8 A, 2026-11-27).

## Not covered

- How long a request id is kept, and its storage shape: decided when scoped.
