---
format: aep.planning-md/3
id: epic:node-export
kind: epic
status: draft
title: Spindle in-process in a Node program
relations:
- serves: vision:spindle
revision: 1
---
# Spindle in-process in a Node program

## Outcome

A Node program loads Spindle's wasm host in-process and serves a specification from it, as an
end-to-end fake backend inside its own HTTP server: no file, socket or thread in the module, state
in memory, several independent instances per process. Requested 2026-11-25 by a consumer, through
the coordinating session. What the consumer asked for:

1. Create an instance from a configuration (specification sources, an environment, an
   `ess-http/1` binding, a decisions file or the answer default, fixed actors with callers, a
   clock start, mint and random seeds), or a refusal with the reason `run` gives before listening.
2. With the instance, the not-served report `run` prints on stderr.
3. Handle one request (method, path, query, headers, body bytes) and answer it (status, headers,
   body bytes) with the router of the native HTTP host, console routes included.
4. Reset, with or without a world, as the reset route does.
5. A clock under the host program's control, and seeded minting and randomness.
6. Event delivery, by callback or pull.
7. Dispose of an instance.

Loadable by Node 22 as an ES module, and built by the release as an artifact with the module's size
in the release notes.

## Stories

- `story:node-export-serves-a-specification` — items 1 (sources, environment, binding, decisions,
  answer default), 2, 3, 4 and 7, several instances, ES module.
- `story:node-export-fixed-actor` — item 1's fixed actors.
- `story:node-export-clock-and-seeds` — item 1's clock start and seeds, item 5.
- `story:node-export-events` — item 6.
- `story:node-export-release-artifact` — the release artifact and its size.

## Boundary

`story:wasm-test-library` (epic:browser-host) stays: a TypeScript library that takes each
capability as a binding supplied from TypeScript (a mocked channel, a faked endpoint). This epic
serves a specification over its routes and takes no capability from JavaScript.
