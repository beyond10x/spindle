---
format: aep.planning-md/3
id: story:seed-under-a-lease
kind: story
status: draft
title: A run with a lease seeds once whatever token it took
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A run on PostgreSQL with `database_lease` set submits its seed when the store holds no record of a
completed seed of that file, whatever lease token it took, and skips it once one process has
completed it: the seed's progress is kept in the component's tables, as `seed.json` keeps it for a
`--state` directory.

## Why

Found by wave 31's U6 adversary: under D5 A the seed runs only while every lease is at its first
token, so with `database_lease` set a restart of the only process, a changed seed file in a later
release, and a first process that died before seeding (exit 3 on a misfit after taking its leases)
never seed again. That narrows `story:seed-at-start`'s "on every start" for runs that set the
lease.

## Acceptance

- A first process exits before seeding; the next, at token 2, seeds and the entries land once.
- A restart after a completed seed with the same file skips it; with another file it is refused as
  a `--state` run refuses it.
