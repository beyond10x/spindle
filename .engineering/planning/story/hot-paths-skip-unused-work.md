---
format: aep.planning-md/3
id: story:hot-paths-skip-unused-work
kind: story
status: implemented
title: Hot paths skip work whose result nothing reads
relations:
- serves: vision:spindle
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T11:26:06Z", actor: "human:timo", revision: 4}
- {from: "proposed", to: "active", at: "2026-11-22T11:26:06Z", actor: "human:timo", revision: 5}
- {from: "active", to: "implemented", at: "2026-11-22T11:26:06Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
---
# Hot paths skip work whose result nothing reads

## Outcome

- `rests` (`crates/spindle-kernel/src/execute.rs:63`) returns before building `Facts::row` when
  the entity declares no invariant. On the library workload of the 0.17.0 review, 240 such
  constructions took 18.0 % of instructions.
- A bulk write on the SQLite store reads the version of every selected row in one query (or one
  batch) instead of one blocking log read per row. In 0.17.0, renewing 24 loans made 48 blocking
  log reads before one atomic append. A row changed by another writer between read and append is
  still a conflict, as the `Store` contract cases (`spindle_kernel::contract`) require.
- `check`, `capabilities` and `admit` do not read the authored scenario files `ess-inputs.yaml`
  lists under `scenarios:`. In 0.17.0, 8 MiB of comments in one scenario made each of the three
  read 8 MiB more with byte-identical output. `run --scenario`, `conform` and every command that
  uses scenarios still reads them.

## Acceptance

- `an_entity_without_invariants_builds_no_facts`.
- `a_bulk_write_reads_versions_in_one_batch`: renewing 24 rows makes a constant number of log
  reads, not one per row; the existing conflict tests and the `Store` contract cases pass.
- `check_capabilities_and_admit_read_no_scenario`: with an unreadable scenario file listed, the
  three commands answer as with a readable one; `conform` still reads it.

## Origin

Spindle 0.17.0 review, 2026-11-22: performance findings 3, 4 and 5.

## Residual

- `metrics`, `play`, `deploy`, `state` and `database` still read scenarios.
- A PostgreSQL bulk write still reads each row: the eventlog batched read cannot ask a stream for
  its version.
