---
format: aep.planning-md/3
id: approval-record:wave12-close-branch
kind: approval-record
status: accepted
title: Wave 12's store close rides the next wave's PR
relations:
- decides: story:outcome-scoped-refusal
- decides: story:view-list-query-parameters
- decides: story:http-outcome-bodies
- decides: story:observe-ingress-model
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T01:59:10Z", actor: "human:timo", revision: 2}
---
# Wave 12's store close is pushed as a branch and rides the next wave's PR

## Words

The orchestrator, relaying the operator's grant, 2026-11-07:

> Store close: push `store/wave12-close` as a branch through the bot now, with no PR. It rides the
> next spindle wave's PR [...]. Reason: a branch that exists only in a local tree is lost when that
> tree is finished. After the push, add the verification report to the branch once verify passes.
> Then finish the integration tree with --discard-cache --archive. Record this in your store.

## What it decides

- Branch `store/wave12-close` (the four wave 12 stories moved to `implemented`, the wave page's
  delivery line, and `verification-report:release-0.4.0` once the release check passes) is pushed
  to the repository through the bot, with no pull request.
- The next spindle wave merges it into its integration branch, and it reaches `main` with that
  wave's PR.
