---
format: aep.planning-md/3
id: story:seed-at-start
kind: story
status: active
title: run applies a seed file of commands in the background at start
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-10T23:01:13Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-10T23:01:14Z", actor: "human:timo", revision: 4}
---
# run applies a seed file of commands in the background at start

## Outcome

`run` takes a seed file (a flag and the matching environment setting) holding an ordered list of
commands `{command, input, actor, caller, request_id}`. After the store is open, without holding
back the listener, Spindle submits them one at a time in the file's order, on every start, and
reports what happened as counts: sent, applied, already-there, refused.

## Why

Asked for by a downstream consumer on 2026-11-10 for a static root seed (people, their roles and
groups), which the operator wants "triggered in the background by Spindle". Today the consumer
sends the seed from its own in-pod process through the ingress on every start, relying on
`existing_instance:` outcomes. Spindle has no seed at start: `POST /api/reset` with a world resets
the whole state and is for tests.

## Acceptance

- `run --seed <file>` (and its environment setting) on a fixture: the listener answers before the
  seed finishes; the seed's commands are decided in the file's order, one at a time, each with the
  file's actor, caller and request id.
- Run twice on one state: the second run applies nothing new; every command whose outcome the
  specification declares for an existing instance (`existing_instance:`) counts as already-there,
  not as refused; a repeated request id is deduplicated as any request is.
- A command refused for any other reason is logged with its command, request id and error name,
  the remaining commands are still submitted, and the process keeps serving.
- The counts (sent, applied, already-there, refused) are visible in `/__spindle/metrics` and in one
  log line when the seed ends.
- On a trusted-caller run the seed needs no proxy secret: it does not come through the listener;
  its actor and caller are the file's.
- A seed file that does not parse, or names a command, field or actor shape the specification does
  not declare, exits 1 before listening, naming the file and the entry.

## Decided at acceptance

- The seed file is one JSON array of `{command, input, actor, caller, request_id}` objects.
- A refusal is not fatal by default; a setting that makes it fatal (exit non-zero) may come with the
  story, off by default.
- Its name in the environment document is decided when scoped. The consumer's command list follows
  from its own specification work; the story does not depend on it.

## Not covered

- Generating the file from a domain seed format: the consumer generates it.
- Ordering or retries across processes (two replicas starting at once): each submits the whole
  file; stable request ids and `existing_instance:` outcomes make that safe.
