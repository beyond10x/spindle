---
format: aep.planning-md/3
id: story:views-with-filters-and-lists
kind: story
status: active
title: Views with filters and lists
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:rally-is-watchable-in-a-terminal
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# Views with filters and lists

## Outcome

The view reader answers list views over every instance of the entity, with `filter: state == <State>` and `order_by: [<field> asc|desc]`, for `read_your_writes` and `eventual` consistency (one process: eventual reads the same store). Parameters, computed fields, aggregation and paging stay refused as Unsupported.

## Acceptance

- `a_list_view_answers_every_instance`
- `a_state_filter_keeps_only_instances_in_that_state`
- `order_by_desc_on_a_timestamp_orders_newest_first`
