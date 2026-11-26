---
format: aep.planning-md/3
id: story:stdin-arrange-and-line-references
kind: story
status: draft
title: A stdin run arranges rows and names what earlier lines minted
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: README.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/lines.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/stdin-lines
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/stdin_lines.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-host/src/world.rs
- confidence: inferred
  path: crates/spindle-kernel/src/mint.rs
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 17
---
# A stdin run arranges rows and names what earlier lines minted

## Outcome

A stdin run takes two more kinds of line beside a command:

- an arrange line that writes stored rows before the next command, as a scenario's `arrange:` and
  `POST /api/reset` `rows` do;
- a command line whose input names an identity an earlier line minted, or an event an earlier line
  published, by a reference to that line's answer.

Today a stdin line is a command only. A consumer feeds Spindle one line at a time over pipes and
resolves references itself from earlier answers, and cannot set up rows at all on stdin.
`--scenario` has `arrange:` and `capture:`, but ESS's scenario rules refuse acts sent as a system
actor, which stdin accepts.

## Acceptance

- `an_arrange_line_writes_rows_a_later_command_reads`: an arrange line with one row, then a command
  guarded on that row, answers the guarded outcome.
- `a_line_names_an_identity_an_earlier_line_minted`: a create line, then an update line whose
  instance is a reference to the create's minted identity, answers the update's outcome.
- `a_reference_to_a_line_not_yet_answered_is_rejected`: a reference to a later line, an unknown
  line or a field the answer lacks rejects the line, and with story:run-fails-on-rejected-line the
  run exits 1.
- `an_arrange_line_refused_by_the_schema_is_rejected`: a row whose fields the entity does not
  declare rejects the line and writes nothing.
- `a_system_actor_command_on_stdin_still_answers`: a command granted only to a system actor, sent
  on stdin, answers as today.

## Not covered

- References across separate runs.
- Changing ESS's scenario authoring rules.

## Origin

A consumer's simulation, 2026-11-26, on 0.21.0 (`run --help`: a stdin line is a command only).

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-cli`, the stdin loop of `run` — cited
- **Files:** `crates/spindle-cli/src/main.rs:45-61` (`Request`, `deny_unknown_fields`: the only line shape today), `:725-736` (the mint, built once before the loop), `:737-794` (the stdin loop: parse, `moved`, `answer`, `rejected` with `index + 1`), `:862-897` (`answer`, which builds the printed answer `{command, outcome, events, error?, response?}`) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:79-100` (`run` help: "Each line is `{"command": ...}`") — cited
- **Symbols:** `Request`, `answer`, `moved`, `held_identities` (main.rs:798), `spindle_host::read_world` (`crates/spindle-host/src/world.rs:67`, row shape `{entity, identity, state, fields}`, ESS `validate_entity_setup` + `Integer` normalising), `spindle_kernel::at_rest` (`crates/spindle-kernel/src/execute.rs:45`), `Store::commit_all` (`crates/spindle-kernel/src/store.rs:250`, all rows or none), `spindle_kernel::Executed` (`crates/spindle-kernel/src/lib.rs:140`: no minted identity field) — cited
- **Also likely:** a new `crates/spindle-cli/src/lines.rs` (line enum, reference resolution, arrange write) so that the loop in main.rs changes in few lines — inferred
- **Also likely:** `crates/spindle-kernel/src/mint.rs:523-570` (`Seeded`): only if D3 = A, a `&mut` way to avoid identities an arrange line wrote, as `Host::reset_to` avoids a world's rows (`crates/spindle-host/src/host.rs:1265`) — inferred
- **Also likely:** `crates/spindle-host/src/world.rs`: only if the implementor exposes a per-row reader instead of calling `read_world` with `{"rows": [...]}` (whose errors say "reading the reset's world") — inferred
- **Tests:** new module `crates/spindle-cli/tests/spindle-cli/stdin_lines.rs` with the five acceptance tests (`an_arrange_line_writes_rows_a_later_command_reads`, `a_line_names_an_identity_an_earlier_line_minted`, `a_reference_to_a_line_not_yet_answered_is_rejected`, `an_arrange_line_refused_by_the_schema_is_rejected`, `a_system_actor_command_on_stdin_still_answers`), each spawning `b10x-spindle run` once on `fixtures/memory.toml` as `rejected_line.rs` does (well under 1 s each, the 60 s per-test bound and the 120 s step budget hold); registered in `tests/spindle-cli/main.rs` — cited (pattern), inferred (module name)
- **Tests:** new fixture `crates/spindle-cli/tests/fixtures/stdin-lines/{system.yaml,domains/<one>.yaml}`: an entity with a guarded command, a create whose event (or response) carries the minted identity, and an actor with `system: true` granted one command. No fixture in the tree declares a system actor today — inferred
- **Documents:** `crates/spindle-docs/src/status.rs` (a new `shipped` row next to `:446` "A rejected line fails the run"), `README.md:55-64` (stdin paragraph), generated `website/docs/reference/cli.md` (from cli.rs via `crates/spindle-docs/src/cli.rs`), `website/docs/status.md`, `website/data/status.json`, `CHANGELOG.md` — cited (files), inferred (that each changes)
- **Not touched:** `crates/spindle-ingress`, `serve.rs`, `seed.rs`, the `--scenario` replay (`replay.rs`, `spindle-conformance`) — inferred
- **Confidence:** high for the landing site (a single stdin loop, a single answer builder, and the row reader already exists); medium for the kernel and host lines, which depend on D1 and D3
- **Would collide with:** any unit that changes `run`'s stdin loop or `answer` in `crates/spindle-cli/src/main.rs:725-897`, the `run` help in `cli.rs:79-100`, the `tests/spindle-cli/main.rs` module list, `spindle-docs/src/status.rs` and the generated status/CLI pages; with D3 = A, any unit changing `Seeded` in `spindle-kernel/src/mint.rs`; with a host row reader, any unit changing `spindle-host/src/world.rs` (`integers`)
- **Safety fact:** an arrange line can write nothing the reset world could not: rows pass ESS's `validate_entity_setup` (`spindle-host/src/world.rs:107`) and `at_rest` invariants before one `commit_all` with `expected: None`, so a bad or already-held row writes no row of the line, and a command line without a reference parses as today because `Request` is unchanged. Level 2 (pointed at `file:line`), unproven
