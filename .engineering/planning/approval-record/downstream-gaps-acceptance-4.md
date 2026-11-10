---
format: aep.planning-md/3
id: approval-record:downstream-gaps-acceptance-4
kind: approval-record
status: accepted
title: Downstream accepts view-filter-reads-now and asks kernel-guard-reads-now for wave 18
tags:
- delegated
relations:
- decides: story:view-filter-reads-now
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-10T04:41:17Z", actor: "human:timo", revision: 2}
---
## Decision

Accepted on 2026-11-10 by the downstream session that owns the consumer, under the operator's
standing rule that downstream sessions accept their own gap stories:

- story:view-filter-reads-now is accepted.
- story:kernel-guard-reads-now (already active) is asked for in the first wave after wave 17. A
  consumer's 5-minute lease on running rows depends on it.
