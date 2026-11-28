---
format: aep.planning-md/3
id: story:scan-cost-test-steady
kind: story
status: implemented
title: The scan cost test passes in a loaded PostgreSQL lane
relations:
- serves: vision:spindle
revision: 4
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T14:03:09Z", actor: "human:operator", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-28T14:03:09Z", actor: "human:operator", revision: 3}
- {from: "active", to: "implemented", at: "2026-11-28T14:03:10Z", actor: "human:operator", revision: 4, decided_on: {"recorded":{"test_result":1}}}
---
## Outcome

`adversary_postgres::adv_a_scan_costs_in_proportion_to_the_log` passes in the PostgreSQL lane while
the other lane tests run beside it: it compares counts of work (rows read, statements sent), not
wall-clock times of two scans.

## Why

Found during wave 31's U8 correction round: in a full PostgreSQL lane run (75.3 s) it failed with
2.1 ms against 6.6 ms, and passed alone in 0.85 s. The round did not change the scan code.

## Acceptance

- The lane passes 10 runs in a row with the test inside it.
- The test still fails when a scan reads the whole log for a short page.
