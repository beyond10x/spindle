---
format: aep.planning-md/3
id: story:view-related-fields
kind: story
status: active
title: A view field read through a reference is served
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:37Z", actor: "human:timo", revision: 3}
---
# A view field read through a reference is served

## Outcome

A view field declared `field: {related: {via: <reference field>, field: <field>}}` is read from the
referenced row. A downstream specification has 26 views with such a field (its count); today they
answer 501 (its measurement).

## Acceptance

On a neutral fixture with a view over `Grant` carrying `resource_name` read through `resource_id`
from `Resource.name`, each row carries the referenced resource's name, the field is absent when the
reference is empty or the row is gone as ESS 0.90.0's interpreter answers, and the answers equal
the interpreter's.
