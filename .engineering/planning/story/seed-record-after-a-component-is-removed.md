---
format: aep.planning-md/3
id: story:seed-record-after-a-component-is-removed
kind: story
status: draft
title: A seed record survives the removal of the component whose lease table held it
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A leased run on PostgreSQL finds its seed record also when a later release removed the component
whose lease table held it, so another seed file is still refused and a part-done seed still resumes.

## Why

Wave 32's U4 (`story:seed-under-a-lease`) keeps the record in the `seed` row of the first
component's `<prefix>_p_lease` table and, after its adversary round 1, reads it from every current
component's lease table, moving it to the first one on the next write. A record kept only in the
table of a component the current specification no longer has is not read: the store holds rows,
no record is found, and D5 B takes the seed as applied (a complete record is written over the
loss). Reaching the old table needs a scan of the schema's `<prefix>_*_p_lease` tables, and moving
the record out of it cannot be guarded by that component's lease, since no run holds it.

## Acceptance

- A lane test: release 1 seeds with component `a` sorting first; release 2 drops `a`; a part-done
  seed resumes after its count, and another seed file exits 1 naming both digests.
- The move out of a table no current component owns is guarded by the first component's lease, and
  a kernel test shows a deposed process cannot make it.
