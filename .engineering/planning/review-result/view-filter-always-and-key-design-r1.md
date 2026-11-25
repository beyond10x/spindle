---
format: aep.planning-md/3
id: review-result:view-filter-always-and-key-design-r1
kind: review-result
status: active
title: Plan critic (design), round 1, on the always and key filter story
relations:
- reviews: story:view-filter-always-and-key
revision: 1
---
approve

No cycle: the story's edges lead to `vision:spindle` and three implemented stories. `story:command-with-unpublished-event-served` touches `crates/spindle-channel/src/channel.rs`, not the view reader. Of the other drafts naming `crates/spindle-host/src/view.rs`, `story:view-rows-absent-optional-fields` owns `project` and `story:view-paging` the paging refusal; this story owns `reads`, `Scope::admits` and `Scope::typed`. The `(always)` leaf and the key path are two arms of one filter reader with one fixture; the acceptance runs from the host reader to the HTTP routes.

```findings
[]
```
