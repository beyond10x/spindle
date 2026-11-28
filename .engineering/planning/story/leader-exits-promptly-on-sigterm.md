---
format: aep.planning-md/3
id: story:leader-exits-promptly-on-sigterm
kind: story
status: draft
title: A leader on PostgreSQL exits promptly on SIGTERM under load
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

A leading process on PostgreSQL that receives SIGTERM releases its leases and exits within a bound
the docs name, also while the store is loaded, so a standby takes over without waiting for the lease
to run out.

## Why

Found by wave 31's U6 implementor: `a_standby_posts_no_webhook_and_keeps_no_dead_letter` failed in
2 of 9 PostgreSQL lane runs because the leader took more than 10 s to exit after SIGTERM (14.9 s in
one probe). At 8 s its database sessions were idle and unblocked, the standby had ended no session,
and the leader's threads were waiting in the process. Bulk-insert transactions of 13 s from other
tests were open on the cluster at the time. The cause inside the process is not found.

## Acceptance

- The in-process wait that holds the exit after SIGTERM is named, with a test that is red first.
- Under a store with long open transactions from another role, a leader exits within the named
  bound after SIGTERM in 20 of 20 runs, and the standby leads within one poll of that exit.
