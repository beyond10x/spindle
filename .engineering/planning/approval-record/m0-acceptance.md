---
format: aep.planning-md/3
id: approval-record:m0-acceptance
kind: approval-record
status: accepted
title: Operator accepts the four M0 stories already shipped
relations:
- decides: story:load-and-compile
- decides: story:derive-capabilities
- decides: story:admit-against-environment
- decides: story:rally-specification
revision: 3
transitions:
- {from: "draft", to: "accepted", at: "2026-11-04T18:28:09Z", actor: "human:timo", revision: 3}
---
# Operator acceptance of the four M0 stories already shipped

## Words

The operator, 2026-11-04, answering "may I move 4 older stories to implemented? Load-and-compile,
derive-capabilities, admit-against-environment and rally-specification are still draft, but their
code and tests are on `main`", with option A "I record your acceptance of the 4 and move them, with
the existing tests as evidence" recommended and B "they stay draft":

> A

## What it accepts

`story:load-and-compile`, `story:derive-capabilities`, `story:admit-against-environment` and
`story:rally-specification`, as shipped: each moves to implemented on the evidence of its named
acceptance tests passing in the gate on `main`.
