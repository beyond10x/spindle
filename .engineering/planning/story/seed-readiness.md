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
revision: 1
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
  are held by a slow external cause, the answer says `running` with the entries left, then
  `ended` with `sent`, `applied`, `already_there` and `refused` equal to the end line's counts.
- `a_readiness_route_answers_200_only_after_the_seed_ends`: a route answers 503 while an entry is
  left and 200 once the seed ended, refused entries included; a run without a seed answers 200
  from the first request.
- `the_seed_state_needs_no_observe`: both answers come without `--observe`.
- `a_trusted_caller_run_guards_the_readiness_route_as_its_console`: on a run with
  `--trusted-caller-header`, the route is guarded as the console routes are, or the story says
  why a probe may read it without the secret.

## Decided when scoped

- Where it is answered: a field of `GET /api/served`, a route of its own under `/__spindle/`, or
  both; and whether a refused entry keeps the route at 503 (a setting, off by default).
