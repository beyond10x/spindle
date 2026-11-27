---
format: aep.planning-md/3
id: review-result:wave30-u12-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 30 (U12)
relations:
- reviews: story:served-clock-steps
revision: 1
---
approve

**What I read:** U12 (`story:served-clock-steps`, full body) and the wave page's U12 changes: the units table, Surfaces, merge order and Selection. I also read the two clock stories next to it in the graph, `story:clock-and-mint-in-every-mode` (implemented) and `story:node-export-clock-and-seeds` (draft, embed and node only), so neither claims the same outcome. I checked against the tree: the five `Console` routes in `console.rs:17-61`, that `spindle-embed/src/lib.rs:175` serves the console, the status entry in `status.rs:473-476`, the README route list at `:72`, and that the three test files and the `clock-served` fixture exist. I ran `aep plan artifact show` for the three stories, `aep plan artifact graph`, and read `.engineering/waves/wave-30.md`.

**Coverage:** I found 11 promises in the parent and traced all 11 to U12.
- Consumer gap: guards that read `now` see the simulated month; event times come from Spindle, not from the driver.
- Outcome:
  - POST moves the clock forward and answers `{"at"}`.
  - The proxy secret guards it.
  - An earlier instant gets 409 and leaves the clock where it stood.
  - Without `--clock` the route answers 409.
  - GET answers the current instant.
  - A reset world's `clock` moves the clock forward.
  - An earlier reset `clock` gets 400 before anything is emptied.
  - Every `/api/events` record carries `at`.
  - `/__spindle/requests` stays unchanged (D4 A, kept by the "Not touched" list).

Periodic bindings are left out on purpose and say so under "Not covered". Nothing reaches past the parent: the docs files in scope are the ones that describe the clock today (the `cli.rs` `--clock` doc, the status "Simulated clock" entry, the README route list). No other story claims the same outcome. The body names no person, host or home path.

**What I could not establish (outside my lane; none of it sets my verdict):**
- Acceptance: the Outcome does not say what `GET /api/clock` answers on a run without `--clock`, or on an embedded host. D1 A gives 409 only for an embedded host without that clock. No test covers the embedded surface, where the route now appears through `with_console`.
- Design: a specification that already declares `/api/clock` will now fail to start, with the console on or off. The story records this as a safety fact, but nothing says the CHANGELOG must name it as a breaking change.
- Parallel safety: the Surfaces section of the wave page still says "`crates/spindle-cli/src/main.rs`, `crates/spindle-cli/src/cli.rs` with the generated `website/docs/reference/cli.md`: U9 only". That now contradicts the U12 entry just above it. Surfaces also does not list `crates/spindle-docs/src/status.rs` or `crates/spindle-ingress/src/console.rs`, so I could not tell whether another unit shares those files.

Files: `<local path>`, story `story:served-clock-steps` in the same worktree.

```findings
[]
```
