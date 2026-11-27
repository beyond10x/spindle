---
format: aep.planning-md/3
id: story:rally-example-reads-stdin
kind: story
status: draft
title: The rally example's stdin run reads its lines
relations:
- serves: vision:spindle
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/rally_page.rs
- confidence: cited
  path: website/docs/examples/rally.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 6
---
## Outcome

The "Drive it by hand" example of `website/docs/examples/rally.md` runs as written: the command it
gives reads JSON lines on stdin and answers them. Today `examples/rally/ess` declares `rally-table`
with `reached_by: network`, so `run` on the laptop environment serves HTTP and never reads stdin; a
probe with `timeout 20` exited 124 after printing only the listening line.

Found by the wave 27 U1 implementor and adversary; the same on the base c61a59650, so pre-existing.

## Acceptance

- `the_rally_example_reads_stdin_as_its_page_says`: the page's command (with `--stdin`), run on the
  example with the page's input lines read from `website/docs/examples/rally.md`, prints the
  page's output block (each answer, then the `caused_by` lines of the bindings each `BallMoved`
  fires) and exits 1, naming line 3 on stderr, as the page says.
- `stdin_on_a_networked_spec_refuses_served_only_flags`: `--stdin` with `--port` exits 2.
- `a_networked_spec_without_stdin_still_serves`: without `--stdin` the example prints its listening
  line, as today.
- The page's sentence that bindings do not fire on stdin is gone.

## Scope

Derived 2026-11-27 by `story-scoper`, against the tree at 394adc71. Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong). Assumes D1 A (`run` flag that reads stdin where a component is `reached_by: network`), D2 A, D3 A.

- **Primary surface:** `crates/spindle-cli` (`run`'s choice between serving HTTP and reading stdin) — cited, read at `main.rs:724-773`
- **Files:** `website/docs/examples/rally.md:56-80` ("Drive it by hand": stale sentence at :59, command at :70, output at :72-74) — cited
- **Files:** `crates/spindle-cli/src/main.rs:724-842` (the `ingress.network` branch and the served-only flag refusals after it) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:151-153` (`run`'s doc: "instead of reading stdin") and the new flag in `Command::Run` — cited
- **Symbols:** `Capability::INGRESS_NETWORK`, `serve::networked`, `lines::Bindings::new` — cited
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/rally_page.rs` (new module: `the_rally_example_reads_stdin_as_its_page_says`, `stdin_on_a_networked_spec_refuses_served_only_flags`, `a_networked_spec_without_stdin_still_serves`) — inferred
- **Also likely:** `crates/spindle-cli/tests/spindle-cli/main.rs` (one `mod` line) — inferred
- **Documents:** `website/docs/reference/cli.md` (generated from clap by `crates/spindle-docs/src/cli.rs`), `CHANGELOG.md` `[Unreleased]` — inferred
- **Confidence:** high for the page and the `run` branch: the story names the page and the tree shows the branch; medium for the flag's files, which depend on D1
- **Would collide with:** any unit touching `run`'s argument set or its doc in `crates/spindle-cli/src/cli.rs` (and so the generated `reference/cli.md`), the non-served path of `run` in `crates/spindle-cli/src/main.rs`, the test module list in `crates/spindle-cli/tests/spindle-cli/main.rs`, or `CHANGELOG.md`
- **Safety fact:** a stdin run of rally fires only event bindings, on one store, as a served run routes them: `Router::build` keeps only what `Route::compile` returns (`crates/spindle-channel/src/route.rs:186-190`), and the periodic `tick` needs the timer that only the served run starts; ESS's interpreter ignores `reached_by` (only synthesis grants read it, `ess-conformance/src/synthesize/grant.rs:198` at 0.99.1). Step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `run --stdin` reads JSON lines even where a component is `reached_by: network`.
- D2 A: the page keeps its third line; the run exits 1 and names line 3 on stderr.
- D3 A: the test reads the command, the input and the expected output from `rally.md`.
