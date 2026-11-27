# Wave 29

**In progress.** Planned by the coordinator on the host operator's order of 2026-11-27, relaying the
operator's words "resolve all missing spindle features". Skill `aep:implementing` 0.26.0, wave mode.
All stories serve `vision:spindle`.

Integration branch `wave/29`, forked from `main` after wave 28's pull request merges and release
0.23.0. Release 0.24.0 follows this wave.

## Selection

A consumer reported four gaps against 0.22.0 on 2026-11-27, each stopping its walking skeleton,
and ranked the escalation first. They were filed as drafts on `plan/wave-28`; on the host
operator's default of 2026-11-27 they form this wave, after wave 28:

- `story:binding-failure-escalates`: `run` refuses a binding that escalates a refusal.
- `story:payload-field-through-conversion`: an outcome field filled through a declared conversion
  is refused.
- `story:value-source-through-two-rows`: a value read through two related rows is refused.
- `story:ess-pin-0-99-2`: the consumer's specification requires ESS 0.99.2.

None changes the architecture under the options taken. The escalation goes to the outbox, not the
store's event log (its D3 A); recording it in the store would change the stored format and is not
in this wave.

The seven stories that wait for the operator's picks and `story:sensitive-fields-redacted` stay out,
as in wave 28.

Each story's `## Scope` was derived against `plan/wave-29`; its `## Decided when scoped` records the
options taken; its `## Outcome` and `## Acceptance` were amended to match them.

## Units

| unit | stories, in order | branch | tree |
|---|---|---|---|
| U1 | `story:binding-failure-escalates` | `unit/w29-escalate` | `spindle-w29-escalate` |
| U2 | `story:payload-field-through-conversion`, `story:value-source-through-two-rows` | `unit/w29-values` | `spindle-w29-values` |
| U3 | `story:ess-pin-0-99-2` | `unit/w29-ess-pin` | `spindle-w29-ess-pin` |

U2's two stories both change `supported::value` (`crates/spindle-kernel/src/supported.rs`) and the
`Sources::value` match in `crates/spindle-kernel/src/values.rs`, so they run one after the other in
one unit. Each unit is built by one `aep:implementor` and gets one `aep:adversary` pass when green.
Builds and tests run through the shared pool; a step that writes into the tree runs as
`slot --fetch <path>`.

## Surfaces

Each story's `## Scope` holds its surface. Shared between units, each on separate hunks and resolved
by the coordinator at merge:

- `crates/spindle-conformance/tests/spindle-conformance/main.rs`: one `mod` line each from U1
  (`escalation`), U2 (`payload_conversions`) and U3 (`input_list_quantifier`).
- `crates/spindle-cli/tests/spindle-cli/conform.rs`: U2 adds
  `conform_runs_an_outcome_with_a_conversion`; U3 may move counts asserted at `:332-410` if
  synthesis on 0.99.2 adds steps. U1's conform case lives in its own module
  `tests/spindle-cli/escalation.rs`.
- `crates/spindle-docs/src/status.rs` with `website/docs/status.md` and `website/data/status.json`
  (U1 and U2, separate rows), regenerated after the merges.
- `CHANGELOG.md` `[Unreleased]`: every unit.
- `Cargo.lock` and the observe generated types (`crates/spindle-observe-api`,
  `ui/src/api/generated`): U3 only.
- `crates/spindle-host`, `crates/spindle-channel`, `crates/spindle-cli/src/lines.rs`,
  `crates/spindle-cli/src/cli.rs`, `crates/spindle-conformance/src/target.rs`,
  `website/docs/reference/cli.md`, `docs/design/spindle.md`: U1 only.
- `crates/spindle-kernel/src`: U2 only.

Merge order: U1, U2, U3. `story:ess-pin-0-99-2` `depends_on` the other three stories: its
acceptance runs their `conform_runs_*` tests on suites synthesised by 0.99.2, so U3 merges `wave/29`
after U1 and U2 merge, runs those tests before it reports green, and merges last.
`story:value-source-through-two-rows` `depends_on` `story:payload-field-through-conversion`: its
chained read reuses the conversion check the first adds.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run) and, where the
adversary found something, a `review_outcome`.

## Commits

The unit commits, the merges into `wave/29`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.24.0 on the host operator's word after the PR merges.
