# Wave 12

Run by the coordinator under `approval-record:wave12-approval` (the wave was named and approved by
the orchestrator, 2026-11-06). Skill `aep:implementing` 0.26.0, wave mode. All four stories serve
`vision:spindle`. The wave plan page of record is [plan-1.md](plan-1.md); waves 2 to 11 ran from
orchestrator relays without a plan page, and so does this one.

Integration branch `wave/12`, forked from `origin/main` 0c6e896, managed tree `spindle-wave12`.
ESS pin 0.88.0, the newest ESS release: no move this wave.

## Units

| unit | story | branch | tree (managed id) | build dir | scratch | starts | stage |
|---|---|---|---|---|---|---|---|
| U1 | `story:outcome-scoped-refusal` | `unit/w12-refusal` | `spindle-w12-refusal` | `<tree>/target` | `<scratch>/u1` | at open | dispatched |
| U2 | `story:view-list-query-parameters` | `unit/w12-listparam` | `spindle-w12-listparam` | `<tree>/target` | `<scratch>/u2` | at open | ba7aa2b green, merged 0f55ecc; adversary pass 1 running |
| U3 | `story:http-outcome-bodies` | `unit/w12-bodies` | `spindle-w12-bodies` | `<tree>/target` | `<scratch>/u3` | after U1 merges (`depends_on`) | waiting |
| U4 | `story:observe-ingress-model` | `unit/w12-observe` | `spindle-w12-observe` | `<tree>/target` | `<scratch>/u4` | at open | fe12f30 green, merged d91fc3a; adversary pass 1 running |

Trees live under the worktree manager's `trees/b10x/spindle/`; `<scratch>` is the coordinator's
session scratch directory for this wave, outside every repository.

Objectives: U1 keeps a command served when one of its outcomes cannot run; U2 reads a view's list
parameter from its repeated query key; U3 serves the three success body kinds an HTTP binding
declares; U4 gives the request log, metric families and topology their home in `spindle.observe`
before any code serves them (the first story of `epic:runtime-observability`).

The three named stories come first, in the order given. U4 fills the fourth place: its surfaces
(`ess/domains/observe.yaml`, `Taskfile.yml`, `crates/spindle-observe-api`, `ui/src/api`,
`docs/design/observe.md`) collide with none of U1 to U3, and the verb places it in its wave 1.
`story:service-worker-serves-a-specification` was the other candidate without a collision; it was
left out because it adds a new web bundle and a headless browser lane, which is more than a fill.

## Surfaces and the split

`crates/spindle-ingress/src/surface.rs` is shared by U1, U2 and U3. U1 and U2 run at once, split by
function: U1 owns the `ess_gen::http::Served::Command` arm of the route build (the outcome loop,
lines 832-860 at 0c6e896) and `answer.rs`; U2 owns the `Served::View` arm (lines 862-892), the
`Queried` type and the query read (lines 150-180). A `git merge-tree` dry run of U1 and U2 runs
before the first of them merges. U3 rewrites the same outcome loop as U1, so it forks from
`wave/12` after U1 has merged.

Verb lines for the wave's stories, verbatim (`aep plan artifact waves --kind story --status
active`, aep 0.79.0):

```
wave 1
  story:observe-ingress-model (inferred)
  story:view-list-query-parameters (inferred)
wave 4
  story:outcome-scoped-refusal (inferred)
wave 5
  story:http-outcome-bodies (inferred)
collision: story:http-outcome-bodies story:outcome-scoped-refusal crates/spindle-ingress/src/answer.rs (inferred)
collision: story:http-outcome-bodies story:outcome-scoped-refusal crates/spindle-ingress/src/surface.rs (inferred)
collision: story:http-outcome-bodies story:view-list-query-parameters crates/spindle-ingress/src/surface.rs (inferred)
collision: story:kernel-related-row-values story:outcome-scoped-refusal crates/spindle-kernel/src/supported.rs (inferred)
collision: story:kernel-row-set-guards story:outcome-scoped-refusal crates/spindle-kernel/src/supported.rs (inferred)
collision: story:kernel-secondary-effects story:outcome-scoped-refusal crates/spindle-kernel/src/supported.rs (inferred)
```

The verb places `story:outcome-scoped-refusal` in its wave 4 only because of the kernel stories
above, none of which is in this wave. Check path: the verb, then the function split above for the
`surface.rs` pairs. Every scope entry of the four stories is `inferred` except
`story:observe-ingress-model`'s (cited, with one inferred fixture directory) and
`story:view-list-query-parameters`'s `surface.rs` (cited).

## Left out, and next in line

- `story:browser-store-several-tabs` (draft) is next in line for the first wave that touches
  `crates/spindle-store-browser`, under the orchestrator's condition on D628: that wave accepts it
  and removes the ignore at `crates/spindle-store-browser/tests/spindle-store-browser/adversary.rs:131`.
  Wave 12 does not touch the crate, so it stays a draft.

## Pre-flight

- `df -h /`: 44G free at open (floor 25G).
- Spindle build directories from earlier waves: none above 100M (`spindle-spec-harden/target` 74M,
  primary checkout `target/` 611M). Trees of waves 1 to 11 are still registered; not touched here.
- Primary checkout: on a stale `main` with an uncommitted `docs/design/spindle.md` that is not this
  wave's; left as it is. The wave forks from `origin/main`.

## What each story's move to `implemented` needs

Evidence against the merge on `main` (CI's Gate run for the wave PR, `test_result`), as waves 10
and 11 recorded it.

## Commits this approval covers

One or more commits per unit on its branch, the merges of U1 to U4 into `wave/12`, fix commits from
the adversary passes, the closing store commit, one PR to `main` through the bot, and the release
the orchestrator's item names. Nothing else.
