---
format: aep.planning-md/3
id: story:decisions-rules-on-input-predicates
kind: story
status: implemented
title: Decision rules match on the command's input
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/decisions.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/adversary_served_decisions.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/decisions.rs
- confidence: cited
  path: crates/spindle-host/src/decisions.rs
- confidence: cited
  path: crates/spindle-node/tests/node/decisions-reset.test.mjs
- confidence: inferred
  path: website/docs/concepts/capabilities.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 24
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T18:23:52Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 23, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "active", to: "implemented", at: "2026-11-26T08:12:33Z", actor: "human:operator", revision: 24, decided_on: {"recorded":{"test_result":1,"review_outcome":20}}}
---
# Decision rules match on the command's input

## Outcome

A decisions file may hold rules that name a command plus equality predicates over its input, and a
served run applies the first rule, in file order, whose predicates all hold for the request. Today
answers key only on command and outcome: `Answers` (`crates/spindle-host/src/decisions.rs:26`)
holds an outcome per command and reads only `question.command` and `question.outcome`
(`:150-156`), and `crates/spindle-cli/src/decisions.rs` re-exports it. Every host (`run`, the Node
export, the Service Worker, a reset's world) parses through `Answers`, so the rules apply in each.

The form, in TOML, beside today's answers:

```toml
[[rules]]
command = "helpdesk.tickets.Escalate"
outcome = "refused"
answer = true
when = { "input.target" = "R" }
```

- `rules` is a top-level array of tables; `flatten` reads it apart from the command tables, and a
  reset's world carries the same array under `decisions.rules` in JSON.
- `when` maps an input path (`input.<field>`, dots for nested struct members) to a string, integer
  or boolean; a rule matches when every entry equals the request's value. An absent optional input
  matches no entry.
- `answer` is the outcome's answer, `true` or `false`, as in today's answers.
- A request no rule matches is answered as today: the command's table, then the top-level answer,
  then `--answer-default`.
- At load, a `when` entry over a field the command's input does not declare, or a value whose type
  is not the field's, is refused naming the rule and the entry, as an undeclared command or outcome
  is refused today (`from_table`, `crates/spindle-host/src/decisions.rs:75-116`).

A reset keeps the run's decisions file: the run's decider is set once at open
(`crates/spindle-host/src/host.rs:574`, `:982`), and a world's own table is laid over it.
`served_decider_survives_a_console_reset`
(`crates/spindle-cli/tests/spindle-cli/adversary_served_decisions.rs:285`) proves it for `run`; no
test proves it for the Node export, and this story adds one.

## Acceptance

- `a_rule_with_an_input_predicate_decides_only_matching_requests`: two requests to one command, one
  matching the rule's `when`, get the two outcomes.
- `the_first_matching_rule_decides`: two rules that both match one request give different
  answers; the request gets the first rule's answer, and with the two rules swapped, the other.
- `a_request_no_rule_matches_takes_the_command_answer`: a command with a rule and a plain answer;
  a request the rule does not match gets the plain answer.
- `a_rule_over_an_absent_optional_input_does_not_match`: a rule whose `when` reads an optional
  input; a request that leaves the input out gets the plain answer.
- `a_reset_world_with_rules_decides_by_them`: a `POST /api/reset` whose world carries
  `decisions.rules`, then a request the world's rule matches, gets that rule's answer.
- A `when` entry over a field the command's input does not declare, or a value of the wrong type,
  is refused at load, naming the rule and the entry.
- `served_decider_survives_a_console_reset` passes unchanged.
- The Node export: `crates/spindle-node/tests/node/decisions-reset.test.mjs` opens `Spindle.create`
  with a decisions file holding a rule, calls `reset()`, then sends a command the rule decides, and
  gets the rule's outcome, in the `node --test` suite.

## Origin

Pre-existing. Requested by a consumer serving a test backend. The same consumer asked that a reset
keep the decisions file it was started with (2026-11-25, relayed by the coordinating session).

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` — cited; `Answers` moved here from `spindle-cli`
- **Files:** `crates/spindle-host/src/decisions.rs:26` (struct `Answers`), `:75` (`from_table`), `:150` (`table`), `:212` (`flatten`) — cited
- **Symbols:** `Answers`, `Answers::parse`, `Answers::from_json`, `Answers::decide`, `Answers::over`, `Answers::served`, `type Answer` — cited
- **The Outcome's citation:** `crates/spindle-cli/src/decisions.rs` now only re-exports `spindle_host::Answers` (line 21); a change there is limited to docs — cited
- **Also likely:** `crates/spindle-cli/src/cli.rs:150-151`, `:219-220` (help text says "never per input value") and the generated `website/docs/reference/cli.md` — inferred
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/decisions.rs` (in-process) and `crates/spindle-cli/tests/spindle-cli/adversary_served_decisions.rs` (served) for the acceptance tests; `crates/spindle-node/tests/node/` for the Node reset test — inferred
- **Documents:** `website/docs/concepts/capabilities.md:65-72` (decisions file format) — inferred
- **Not touched:** `crates/spindle-embed/src/lib.rs`, `crates/spindle-node/src`, `crates/spindle-wasm`, `crates/spindle-host/src/world.rs` pass text or JSON to `Answers` unchanged — inferred
- **Reset:** the run's decider is set once at open (`crates/spindle-host/src/host.rs:574`, `:982`); `reset_to` replaces only the world's answers and the mint (`host.rs:1236-1240`); a world's table is laid over the run's answers (`crates/spindle-cli/src/decisions.rs:128`, `crates/spindle-embed/src/lib.rs:211-214`). No test proves the file still answers after a reset — cited
- **Confidence:** high — parse and apply sites read in the tree; every host reaches one `Answers`
- **Would collide with:** any unit touching `crates/spindle-host/src/decisions.rs`, or `crates/spindle-cli/src/cli.rs` help text and the generated CLI reference
- **Safety:** the kernel passes the command's input to every decider (`External.input`, `crates/spindle-kernel/src/decider.rs:21`, filled at `crates/spindle-kernel/src/execute.rs:1543`); the workspace `toml` has no `preserve_order`, so "first matching rule" needs an ordered form such as an array of rule tables — cited

## Shared files

- `crates/spindle-cli/src/cli.rs` (the `--decisions` help at `:150-151`, `:219-220`) and the
  generated `website/docs/reference/cli.md`: shared with `story:reset-reapplies-seed` (the `--seed`
  help at `:311`); separate hunks, merged by the coordinator, the page generated again after both.
- `crates/spindle-node/tests/node/`: this story adds `decisions-reset.test.mjs` and leaves
  `exchanges.test.mjs` and `world-exchanges.json` to `story:world-row-integer-fields`.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story of wave 25, merged by the coordinator.
