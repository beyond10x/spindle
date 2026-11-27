---
format: aep.planning-md/3
id: story:postgres-lane-runs-locally
kind: story
status: draft
title: The PostgreSQL lane starts its own cluster with the CI's options
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`task check:postgres` starts its own throwaway PostgreSQL 17 cluster when no database is given,
with the same server options as the CI job (`.github/workflows/ci.yml`, the postgres step: TLS,
`max_locks_per_transaction=256`, `max_connections=300`), and removes it afterwards. One sh script in
the repository holds those options and both the task and the CI job read them.

## Why

Wave 31 ran the PostgreSQL lane locally through a harness outside the repository. Its options
drifted from CI's: at 147 cases the local cluster still had the default 100 connections and 2 of 3
lane runs failed on "too many clients already" while CI had been raised to 300. The rule "the local
gate equals the forge" asks for runner differences to be written in the repository.

## Acceptance

- On a host with `initdb` and `pg_ctl` on `PATH` and no database variable set, `task check:postgres`
  passes and leaves no cluster process or directory behind.
- A test reads the server options from the script and from the CI workflow and fails when they
  differ.
