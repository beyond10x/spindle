---
format: aep.planning-md/3
id: story:seed-readiness
kind: story
status: draft
title: A served run says over HTTP when its seed has ended
summary: A served run answers over HTTP whether its seed is running or ended, with its counts, so a readiness probe needs no view rows.
relations:
- serves: vision:spindle
- informed_by: story:seed-at-start
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Taskfile.yml
- confidence: cited
  path: crates/spindle-cli/src/seed.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/seed/readiness.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-ingress/src/lib.rs
- confidence: inferred
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: inferred
  path: crates/spindle-ingress/src/router.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-observe-api/types.rs
- confidence: inferred
  path: ess/domains/observe.yaml
- confidence: inferred
  path: ess/fixtures/observe/seed
- confidence: inferred
  path: ess/generated/jsonschema/schema/types
- confidence: inferred
  path: ui/src/api/generated
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/environment-and-admission.md
- confidence: inferred
  path: website/docs/concepts/observe.md
- confidence: inferred
  path: website/docs/status.md
revision: 6
---
# A served run says over HTTP when its seed has ended

## Outcome

A served `run` with a seed answers over HTTP whether the seed is still running or has ended, with
its counts, so a container healthcheck or an orchestrator's readiness probe can wait for it
without counting view rows. Without `--observe` too.

## Why

A downstream consumer running a specification on 0.19.1 in a container has no readiness signal
for the seed (`story:seed-at-start`): its healthcheck counts the rows of a view the seed fills.
What exists today: the seed's end is one line on stderr (`crates/spindle-cli/src/seed.rs`,
`Seeding::step`), and its counts are on the observe recorder only with `--observe`, as
`spindle_seed_commands_total{counted}` in `GET /__spindle/metrics`
(`crates/spindle-ingress/src/observe.rs`, `Recorder::seeded`). The metric holds no total and no
"ended", so a probe cannot tell a seed that ended from one still running, and a run without
`--observe` answers nothing.

## Acceptance

- `the_seed_state_is_answered_while_it_runs_and_once_it_ends`: on a run with a seed whose entries
  are held by a slow external cause, `GET /__spindle/seed` answers `running` with the entries left,
  then `ended` with `sent`, `applied`, `already_there`, `refused` and `rejected` equal to the end
  line's counts.
- `a_readiness_route_answers_200_only_after_the_seed_ends`: `GET /__spindle/ready` answers `503`
  while an entry is left and `200` once the seed ended, declared refusals included; a run without
  a seed answers `200` and `{"state": "none"}` from the first request.
- `the_seed_state_needs_no_observe`: both answers come without `--observe`.
- `a_trusted_caller_run_guards_the_readiness_route_as_its_console`: on a run with
  `--trusted-caller-header`, both routes answer `403` without the proxy secret and as above with it.
- `a_reset_that_seeds_again_answers_running_until_it_ends`: during a reset's seed the routes answer
  `running` and `503`, with that seed's counts, until it ends.
- `a_seed_already_run_on_a_kept_state_is_ended_from_the_first_request`: on a `--state` whose seed
  marker records every entry done, the first request reads `ended` and `200`.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible, unless noted.

- D1 A: `GET /__spindle/seed` answers `{state: none|running|ended, entries, left, sent,
  applied, already_there, refused, rejected}`, `GET /__spindle/ready` answers 503 or 200; both are
  answered without `--observe` and without the host thread; a run without a seed answers
  `{"state": "none"}` and 200.
- D2 A: both routes are guarded on a `--trusted-caller-header` run as every
  `/__spindle/` path is today; a probe presents the secret.
- D3 B: no setting; a declared refusal leaves readiness at 200, and a rejected entry
  already ends the run with exit 1.
- D4 A: during a reset's seed the routes answer `running` and 503 until that seed
  ends, with that seed's counts.

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong). It assumes the defaults of D1 to D4 below.

- **Primary surface:** `crates/spindle-ingress` (the route) and `crates/spindle-cli` (the seed state) — cited, the story names `observe.rs` and `seed.rs`
- **Files:** `crates/spindle-cli/src/seed.rs` (`Seeding::new` 458, `restart` 555, `end` 586, `step` 626, `note` 697) — cited
- **Files:** `crates/spindle-ingress/src/observe.rs` (`Reserved` 82-112, `SeedCounts` 249, `reserved()` 1227-1263, which answers 404 for every reserved path without a recorder at 1235-1237) — cited
- **Symbols:** `Seeding::step`, `Recorder::seeded`, `SeedCounts`, `Reserved::of`, `observe::reserved` — cited
- **Also likely:** `crates/spindle-ingress/src/surface.rs` (a `Surface::with_seed` builder beside `observing` 1468 / `recorder` 1487) — inferred, where the shared seed-state handle is held
- **Also likely:** `crates/spindle-ingress/src/listener.rs` (`answer` 484-533) and `crates/spindle-ingress/src/router.rs` (`routing` 123-140), the two callers of `observe::reserved` — inferred
- **Also likely:** `crates/spindle-ingress/src/lib.rs` (re-export; the module doc at 52-69 says every prefix path is 404 without a recorder) — inferred
- **Also likely:** `crates/spindle-cli/src/serve.rs` (surface built 367-371, `seed::begin` 428-436) — inferred, where the handle is created and given to both sides
- **Tests:** new module `crates/spindle-cli/tests/spindle-cli/seed/readiness.rs`, declared in the `mod` list of `crates/spindle-cli/tests/spindle-cli/seed.rs:1225-1230`; it reuses `held_desk`, `serve`, `exchange` and `environment` from the parent — inferred
- **Tests:** `the_seed_state_is_answered_while_it_runs_and_once_it_ends`, `a_readiness_route_answers_200_only_after_the_seed_ends`, `the_seed_state_needs_no_observe`, `a_trusted_caller_run_guards_the_readiness_route_as_its_console` (all four from the story), plus `a_reset_that_seeds_again_answers_running_until_it_ends` (D4) and `a_seed_already_run_on_a_kept_state_is_ended_from_the_first_request` (marker case) — inferred, each a spawned `b10x-spindle run`, unmeasured, expected a few seconds each; nextest kills one at 60 s (`.config/nextest.toml`); time each before adding, step budget 120 s
- **Tests:** a unit test in `observe.rs`'s test module, `the_seed_and_ready_routes_answer_without_a_recorder`, under 1 s — inferred
- **Documents:** `ess/domains/observe.yaml` (new `spindle.observe.SeedState`, as `AGENTS.md` row `ess/` asks for every answer under `/__spindle/`), `Taskfile.yml` (`OBSERVE_ROOTS` line 8, fixture loop 297), the generated `ess/generated/jsonschema/schema/types/`, `crates/spindle-observe-api/types.rs`, `ui/src/api/generated/`, a fixture `ess/fixtures/observe/seed/` — inferred
- **Documents:** `website/docs/concepts/environment-and-admission.md` (§ A seed submitted at start, 153-240), `website/docs/concepts/observe.md`, `crates/spindle-docs/src/status.rs` (seed item 446) with the generated `website/docs/status.md` and `website/data/status.json`, `AGENTS.md` (rows 38, 40), `CHANGELOG.md` — inferred
- **Confidence:** high for `seed.rs`/`observe.rs`, cited and read; medium for the rest, which follows D1
- **Would collide with:** the `/__spindle/` reserved-route surface (`Reserved`, `observe::reserved`, `listener.rs::answer`); `observe.rs` as a whole; `serve.rs`'s surface build and serve loop; `seed.rs`; the `OBSERVE_ROOTS`/fixture lines of `Taskfile.yml`; `ess/domains/observe.yaml` and its generated projections; `status.rs`, `AGENTS.md`, `CHANGELOG.md`
- **Edges:** `story:health-and-graceful-drain` already `depends_on` this story (cited); this story needs no new `depends_on` (inferred); with D1 = B it would interact with `story:console-routes-off`, which turns `/api/served` off (cited from that story)
- **Architecture:** no new crate, process or external dependency; a new route under the existing `/__spindle/` prefix with a declared JSON answer, the same kind of addition `/__spindle/topology` was — inferred
- **Safety fact:** under `--trusted-caller-header`, every `/__spindle/` path is already refused 403 without the proxy secret, before the reserved route is read (`listener.rs` `answer`: `reserved.is_some() && unproxied`; `router.rs:123`), so D2 = A needs no guard code, only a test — step 2, unproven
- **Safety fact:** `Reserved` is matched only inside `spindle-ingress` (`git grep 'Reserved::'`: `observe.rs`, `listener.rs:72`, `router.rs`, `surface.rs:1431,1622`), so new variants change no other crate — step 2, unproven
- **Safety fact:** the shared state must flip to `ended` only after `Host::submit_as` has returned (`Seeding::step` → `note`), so a probe that reads 200 then reads views holding the last entry; that the host's `Reader` sees that commit by then is not proven — step 2, unproven
