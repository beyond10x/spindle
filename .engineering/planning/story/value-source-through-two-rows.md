---
format: aep.planning-md/3
id: story:value-source-through-two-rows
kind: story
status: active
title: A value source read through two related rows runs
relations:
- serves: vision:spindle
- informed_by: story:related-row-selected-by-predicate
- depends_on: story:payload-field-through-conversion
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/conform.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-kernel/src/supported.rs
- confidence: cited
  path: crates/spindle-kernel/src/values.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/fixtures/related-chain
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/main.rs
- confidence: inferred
  path: crates/spindle-kernel/tests/spindle-kernel/related_chain_values.rs
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/status.md
revision: 12
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T08:24:13Z", actor: "human:operator", revision: 11, decided_on: {"recorded":{"review_outcome":5}}}
- {from: "proposed", to: "active", at: "2026-11-27T08:24:13Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":5}}}
---
## Outcome

A value source that reads a field of a row reached through another row (input id → row A's foreign
key → row B's field) runs in `run` and `conform`, as ESS 0.99.1's interpreter reads it. Today it is
refused with "unsupported: the value source <B.field> of the row <A.fk> of the row input.<id>
names" (`crates/spindle-kernel/src/supported.rs:928`).

## Why

Reported by a consumer on 2026-11-27 against 0.22.0: the outcome that creates an instance reads a
list field of a formation through the workload the input names, and its walking skeleton needs it.
`story:kernel-related-row-values` and `story:related-row-selected-by-predicate` read one related
row, not a chain of two.

## Acceptance

- `a_value_source_through_two_rows_reads_the_far_row`: a neutral fixture with three entities, where
  an outcome copies a `List<String>` field and a state of the row two hops from the input; the command answers,
  the row holds the far row's values, and ESS's interpreter gives the same row. It is refused on
  0.22.0.
- `a_missing_row_in_the_chain_is_answered_as_ess_answers_it`: with the first row absent, with the
  far row absent, and with an absent `Optional` reference on the way, the command is answered as
  ESS's interpreter answers it.
- `a_chain_through_another_component_s_row_is_checked_at_every_hop`: where the middle row's entity
  belongs to another component, the read is refused or answered as the one-hop read of that entity
  is.
- `a_chained_read_through_a_conversion_is_checked_as_a_one_hop_read`: a far-row field filled
  through a conversion into a target whose constraint its value breaks is refused as the one-hop
  read of the same field is, and nothing is written.
- `conform_runs_a_value_source_through_two_rows`: `conform` on the fixture executes every scenario
  that selects the chained outcome (none is skipped as `unsupported`; on 0.22.0 each is), and every
  executed scenario passes.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-kernel` (`values.rs`, `supported.rs`) — cited
- **Files:** `crates/spindle-kernel/src/supported.rs:928` (the refusal named in the story, text
  matches ESS `ResolvedPayloadValue::describe` for a `through` chain) and `:898-912` (the arm that
  admits `RelatedField`/`RelatedState` only where `through.is_empty()`) — cited
- **Files:** `crates/spindle-kernel/src/values.rs:41-117` (`related`, the pre-outcome load of the
  rows; filters `through.is_empty()` at `:72`), `:370-384` (`Sources::value` arms, same filter),
  `:482-508` (`Sources::related`, one row only) — cited
- **Files:** `crates/spindle-kernel/src/supported.rs:296-338` (`foreign_read`): checks only the last
  `entity`, not each `ResolvedRelatedHop::entity`; must check the middle row's entity too once the
  chain runs — cited (read), the need is inferred
- **Symbols:** `ResolvedPayloadValue::RelatedField { through }`, `ResolvedPayloadValue::RelatedState
  { through }`, `ResolvedRelatedHop { entity, field, type_ref }`, `values::Related`,
  `values::related`, `Sources::related`, `supported::value`, `supported::foreign_read` — cited
- **Parity target:** ESS 0.99.1 `interpret::execute::determined::Reads::related` (the chain walk,
  `Reads::row`, `stored_reference`): an absent `Optional` reference gives an absent value; a present
  reference that names no row gives `Undetermined::NoValue` (a capability gap, which ESS reports
  as `unsupported`); a hop field that is the identity field reads the address itself — cited
- **Tests:** new module `crates/spindle-kernel/tests/spindle-kernel/related_chain_values.rs` with
  `a_value_source_through_two_rows_reads_the_far_row`,
  `a_missing_row_in_the_chain_is_answered_as_ess_answers_it` (kernel `no_value` against the
  interpreter determining nothing, the `interpreter_determines` pattern of
  `where_selected_values.rs:93`), `a_chain_through_another_component_s_row_is_checked_at_every_hop`
  and `a_chained_read_through_a_conversion_is_checked_as_a_one_hop_read`; one `mod` line in
  `tests/spindle-kernel/main.rs` — inferred (module name)
- **Tests:** `crates/spindle-cli/tests/spindle-cli/conform.rs`,
  `conform_runs_a_value_source_through_two_rows`, reads the fixture below — inferred
- **Fixture:** new `crates/spindle-kernel/tests/fixtures/<three-entity fixture>/` (`system.yaml`,
  `domains/*.yaml`) — inferred
- **Also likely:** `crates/spindle-kernel/src/execute.rs:363` stays unchanged if `values::related`
  keeps its signature — inferred
- **Documents:** `CHANGELOG.md` `[Unreleased]` (0.19-era line 1325 says "chains … stay refused by
  name"); `crates/spindle-docs/src/status.rs` with `website/data/status.json` and
  `website/docs/status.md` if a row is added (precedent: commit 5db28f60) — inferred
- **Confidence:** high — the story names the refusal site, and the code that must change sits in two
  functions already gated on `through.is_empty()`
- **Would collide with:** any unit changing `supported::value` or `Sources::value` in the kernel
  (an outcome-field conversion lifts its refusal at `supported.rs:839-843`, about 90 lines from this
  arm, in the same function); the kernel test `main.rs` `mod` list; `CHANGELOG.md` `[Unreleased]`;
  the status rows in `spindle-docs/src/status.rs` and its generated files
- **Safety fact:** every row the chain reads is loaded in `values::related` before the outcome
  writes anything (`execute.rs:363`, `values.rs:41`), so the far row is read from the store as it
  was before the outcome, as ESS's `Reads::original` is — step 2, unproven

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: a field and a state read through a chain that starts at an input field; the
  cross-component check reads every entity of the chain.
- D2 A: a `CHANGELOG.md` line and a status row, regenerated.
- D3 A: the missing-row test covers an absent first row, an absent far row and an absent
  `Optional` hop, each answered as ESS's interpreter answers it.
- D4 A: a chained read whose far field carries a conversion gets the same source and
  target check as a one-hop row read (ESS `determined::checked_read` on every row read); it builds
  on the check `story:payload-field-through-conversion` adds, which merges first in U2.
