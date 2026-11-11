---
format: aep.planning-md/3
id: review-result:w19-u1-adversary-1
kind: review-result
status: active
title: Wave 19 U1 adversary pass 1
relations:
- reviews: story:postgres-store
revision: 1
---
needs-revision

```
unit: wave 19 U1 story:postgres-store, commit 3c19fb33 (uncommitted test files only)
verdict: NEEDS-CHANGE
cases: executed 710→723, red 6
origin: introduced 5 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-kernel/tests/spindle-kernel/adversary_postgres.rs` and `crates/spindle-cli/tests/spindle-cli/adversary_postgres.rs`. Red:

- `adv_a_scan_answers_while_another_database_holds_a_write_transaction_open`: the scan of a committed row fails after 10.0 s, "the feed shows 0 of the 1 writes ... a transaction older than the last of them is still open".
- `adv_a_scan_costs_in_proportion_to_the_log`: 40,000 events 2.69 s per scan, 80,000 events 10.56 s.
- `adv_a_refused_certificate_names_database_ca`: a wrong CA reads "PostgreSQL connection unavailable".
- `adv_a_served_run_answers_while_another_database_holds_a_write_transaction`: metrics 500 after 10.0 s, `MatchState` 501 after 19.7 s.
- `adv_a_serving_role_whose_limit_is_the_budget_is_served_with_some_pool`: role limit 8 and `database_connections` 8 are refused at pools 4, 7 and 8.
- `adv_a_malformed_setting_exits_1_naming_it_and_never_the_password`: `database_schema = "Not A Schema"` is reported as a non-loopback host.

Suite: gated serial (`--test-threads=1 postgres::`) kernel 9 passed 3 failed, cli 8 passed 4 failed; all of the unit's own cases pass. In parallel, the unit's own hosted-database case fails while an adversary case holds a transaction on the same server.

Attacked and not broken: two stores on one prefix (one of two concurrent creations lands, 20 of 20 rounds); four concurrent migrations; a foreign CA bundle and an unnamed host are refused; no refusal or debug output prints the password; `database_pool` `0` and `four` and `database_connections` `-3` are refused by name; the wasm32 tree has no tokio-postgres, rustls or eventlog-postgres; component prefixes are distinct; a state directory written by the base build answers the same.

```findings
- file: crates/spindle-kernel/src/sqlite.rs
  line: 305
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: any write transaction open over 10 s in any database of the server makes every scan fail with 500/501 and blocks the host thread 10 s per scan, because the scan reads eventlog's server-wide xmin-watermarked feed
- file: crates/spindle-kernel/src/sqlite.rs
  line: 331
  category: property
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: scan cost is quadratic in log size on PostgreSQL (40k events 2.7 s, 80k 10.6 s) because each 500-event feed page runs a full-table first_unsettled subquery
- file: website/docs/concepts/environment-and-admission.md
  line: 129
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: database_connections described as the serving role's budget cannot equal the role's CONNECTION LIMIT for any pool, so the story's downstream setup (limit 8, budget 8) is refused at every pool
- file: crates/spindle-kernel/src/postgres.rs
  line: 194
  category: boundary
  severity: note
  verdict: NEEDS-CHANGE
  origin: introduced
  message: an invalid database_schema on a loopback URL is reported as a non-loopback host error and never names database_schema
- file: crates/spindle-kernel/src/postgres.rs
  line: 226
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a wrong database_ca surfaces only as "PostgreSQL connection unavailable", the same message as a host that is down
```
