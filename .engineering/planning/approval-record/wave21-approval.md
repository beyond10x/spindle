---
format: aep.planning-md/3
id: approval-record:wave21-approval
kind: approval-record
status: accepted
title: 'Wave 21: performance and security fixes in one integration branch'
tags:
- operator
relations:
- decides: story:view-read-without-log-scan
- decides: story:metrics-observed-series
- decides: story:postgres-store
- decides: story:state-moves-to-postgres
- decides: story:command-grant-before-request
- decides: story:unauthenticated-grant-refusals
- decides: story:refusal-reason-bounded
- decides: story:state-directory-lock
- decides: story:view-related-fields
- decides: story:kernel-value-expressions
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-11T09:40:13Z", actor: "human:timo", revision: 2}
---
# Wave 21: performance and security fixes in one integration branch

## Who decided

The operator, in the coordinating session on 2026-11-11: "take a fair batch of fixes, especially
everything performance and security related, should be handled with high priority, make all of
that a whole wave, meaning everything ends up in one integration branch and you only run very
expensive gates or the more expensive gates only at the end. Don't waste time with constant full
gate checking. ... And then eventually create one PR, auto merge and cut a new version."

A downstream consumer also asked on 2026-11-11 that `story:metrics-observed-series` be accepted.

## What it decides

- The wave is these stories, moved to active where they were drafts:
  - performance: `story:view-read-without-log-scan`, `story:metrics-observed-series`,
    `story:postgres-store` with `story:state-moves-to-postgres` (batched reads);
  - security: `story:command-grant-before-request`, `story:unauthenticated-grant-refusals`,
    `story:refusal-reason-bounded`, `story:state-directory-lock`;
  - downstream 501s: `story:view-related-fields`, `story:kernel-value-expressions`.
- Every unit merges into one integration branch, `wave/w21`. Units run the tests of what they
  touch; the full gate (`task check` and the PostgreSQL lanes) runs once, on the integration
  branch, after the adversary fix pass.
- One PR from `wave/w21`, merged on green, then release 0.14.0.

## Commits this approves

- One commit per unit on its unit branch, merge commits into `wave/w21`, fix-pass commits, the
  store records, and `chore: release 0.14.0`; the tag `0.14.0` on the merge commit.
