# Wave 30

**Proposed.** Planned by the coordinator on the host operator's order of 2026-11-27, relaying the
operator's words "make sure spindle is moving with its last planned features, must continue to be
developed". Skill `aep:implementing` 0.26.0, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/30`, forked from `main` after wave 29's pull request merges. Release 0.25.0
follows this wave.

## Selection

Wave 28 delivered the consumer stories of the production service; wave 29 the four gaps a consumer
reported against 0.22.0. Seven stories that would change the architecture wait for the operator's
pick, and `story:sensitive-fields-redacted` waits for an ESS release. From the rest of the backlog
the coordinator ranked first where Spindle answers differently from ESS's interpreter, then
correctness of Spindle's own APIs, then a flaky test and a stale example. The host operator chose
this list on 2026-11-27 (D-W30 A):

- `story:view-rows-absent-optional-fields`, `story:view-filter-nested-element-quantifier`,
  `story:view-filter-map-value-order`: view rows and view filters as ESS answers them.
- `story:error-payload-generated-fields`, `story:unknown-instance-unsourced-unserved`: error
  payloads as ESS draws and serves them.
- `story:create-on-held-identity-unavailable`, `story:decimal-identity-any-spelling`: identities as
  ESS holds them.
- `story:look-ahead-asks-only-where-it-matters`: the kernel asks a decider only where the answer
  matters.
- `story:related-via-nested-input`: pinned as ESS's rejection (its D1 B); no kernel change.
- `story:events-cursor-across-restart`: the events cursor reads what a restarted host commits.
- `story:peer-connect-test-steady`: the slow-peer test passes on a loaded runner.
- `story:rally-example-reads-stdin`: the rally example runs as its page says.

None changes the architecture under the options taken. Each story's `## Scope` was derived against
`plan/wave-30` (main at 0.22.0 plus wave 28); its `## Decided when scoped` records the options
taken; its `## Outcome` and `## Acceptance` were amended where the scoper found the draft stale.

## Units

| unit | stories, in order | branch | tree |
|---|---|---|---|
| U1 | `story:view-filter-nested-element-quantifier`, `story:view-filter-map-value-order` | `unit/w30-view-filters` | `spindle-w30-view-filters` |
| U2 | `story:view-rows-absent-optional-fields` | `unit/w30-view-rows` | `spindle-w30-view-rows` |
| U3 | `story:unknown-instance-unsourced-unserved`, `story:error-payload-generated-fields` | `unit/w30-error-fields` | `spindle-w30-error-fields` |
| U4 | `story:look-ahead-asks-only-where-it-matters` | `unit/w30-look-ahead` | `spindle-w30-look-ahead` |
| U5 | `story:create-on-held-identity-unavailable` | `unit/w30-held` | `spindle-w30-held` |
| U6 | `story:decimal-identity-any-spelling` | `unit/w30-decimal` | `spindle-w30-decimal` |
| U7 | `story:events-cursor-across-restart` | `unit/w30-cursor` | `spindle-w30-cursor` |
| U8 | `story:peer-connect-test-steady` | `unit/w30-peer` | `spindle-w30-peer` |
| U9 | `story:rally-example-reads-stdin` | `unit/w30-rally` | `spindle-w30-rally` |
| U10 | `story:related-via-nested-input` | `unit/w30-via` | `spindle-w30-via` |

U1's two stories rewrite the same `Exists`/`Forall` arm of `Scope::admits` and U3's two stories the
same `unsourced_error_fields`; each pair runs one after the other in one unit, and the second story
of each `depends_on` the first. Each unit is built by one `aep:implementor` and gets one
`aep:adversary` pass when green.

## Surfaces

Each story's `## Scope` holds its surface. Shared between units, each on separate hunks and resolved
by the coordinator at merge:

- `crates/spindle-kernel/src/execute.rs`: U3 the error path (`error_of`, `unsourced_error_fields`,
  doc only), U4 `look_ahead`, `select_ahead`, `ahead_answer` (`:1580-1712`), U5
  `creation_identity` (`:2076-2136`), U6 `reaching` (`:271-345`).
- `crates/spindle-kernel/src/lib.rs`: U5 the field on `ExecuteError::Undeclared`, U6 a re-export.
- `crates/spindle-kernel/src/supported.rs`: U3 only (`unfilled_error_field`).
- `crates/spindle-conformance/src/target.rs`: U2 `query_view` (`:461`), U5 the undeclared mapping
  (`:377-378`).
- `crates/spindle-host/src/view.rs`: U1 the filter admission; U2 at most a doc comment of
  `project`.
- `crates/spindle-host/src/host.rs`: U6 `Host::establish` (`:1271-1281`); U7 the doc comments of
  `published_after` and `published_head` (`:1338-1368`).
- `crates/spindle-host/src/host/component.rs`: U8 only (`Host::connect`).
- `crates/spindle-cli/src/main.rs`, `crates/spindle-cli/src/cli.rs` with the generated
  `website/docs/reference/cli.md`: U9 only (`--stdin`).
- `crates/spindle-cli/src/replay.rs`: U5 only (`, unavailable`).
- The test `main.rs` module lists of `spindle-kernel` (U3, U6), `spindle-conformance` (U2),
  `spindle-ingress` (U3), `spindle-cli` (U9); `README.md` (U3); `CHANGELOG.md` `[Unreleased]`
  (every unit).

Merge order: U10, U8, U7, U1, U2, U4, U3, U5, U6, U9. The `execute.rs` units merge U4, U3, U5, U6;
U6 last of them, as its wrapper sits at the entry of every command.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run) and, where the
adversary found something, a `review_outcome`.

## Commits

The unit commits, the merges into `wave/30`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.25.0 on the host operator's word after the PR merges.
