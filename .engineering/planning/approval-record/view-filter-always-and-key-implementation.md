---
format: aep.planning-md/3
id: approval-record:view-filter-always-and-key-implementation
kind: approval-record
status: accepted
title: The coordinating session orders the always and key filter story implemented
tags:
- coordinator
relations:
- decides: story:view-filter-always-and-key
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-25T10:08:44Z", actor: "human:operator", revision: 2}
---
# The coordinating session's order to implement the always and key filter story

## Who decided

The coordinating session, 2026-11-25, in its brief to the implementing agent: implement
`story:view-filter-always-and-key` for a consumer whose 8 routes and 193 journeys answer 501,
plan it through the store, move it to active the way the store's flow requires, red tests first,
in one pull request with two further draft stories and a note in `AGENTS.md`, not merged and not
tagged.

This is the coordinating session's order as the implementing agent received it, not a quote of the
operator. Nothing here is the operator's approval; the pull request is the operator's to merge.

## The critics

Two rounds of the four plan critics (`aep:plan-critic-acceptance`, `-design`, `-scope`,
`-parallel-safety`) over the story:

- Round 1: design `approve`; acceptance, scope and parallel-safety `needs-revision`
  (`review-result:view-filter-always-and-key-*-r1`), every finding fixed in the body and the
  fixture.
- Round 2: acceptance, scope and parallel-safety `needs-revision` with warnings only
  (`review-result:view-filter-always-and-key-*-r2`): a refusal scenario for another leaf, the
  nested and `Optional` struct paths proved by a view, the reads named, and two more stories
  sharing files. Each was fixed in the body and the fixture after the round, as recorded by
  `review_outcome` on the story.

## What it decides

- `story:view-filter-always-and-key` moves to active and is implemented on
  `feat/view-filter-always-and-key`.
- `story:command-with-unpublished-event-served` and `story:view-field-wire-names` stay drafts.
