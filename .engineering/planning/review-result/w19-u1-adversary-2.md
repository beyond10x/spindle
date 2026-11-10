---
format: aep.planning-md/3
id: review-result:w19-u1-adversary-2
kind: review-result
status: active
title: Wave 19 U1 adversary pass 2
relations:
- reviews: story:postgres-store
revision: 1
---
needs-revision

```
unit: wave 19 U1 story:postgres-store, 0a690813 (fix pass 1 on 3c19fb33), uncommitted test files only
verdict: NEEDS-CHANGE
cases: executed 290→302, red 7 (debug; 5 in release)
origin: introduced 4 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-kernel/tests/spindle-kernel/adversary_postgres_scan.rs`. Red in release:

- `adv2_a_scans_memory_does_not_grow_with_the_log`: 220 MiB at 100k events, 1062 MiB at 500k; SQLite grows 2 MiB.
- `adv2_an_append_from_another_process_lands_during_a_long_scan`: log 400k, scans about 4 s; 3 of 16 commits from a second store failed after 2.00 s with "event store deadline exceeded during database statement or lock".
- `adv2_two_processes_scan_one_long_log_at_once`: log 300k; the second scan fails at 2.003 s.
- `adv2_no_refusal_prints_a_password_holding_an_unencoded_at`: with `database_ca`, "no TLS connection to PostgreSQL at retpart@db.example.invalid:5432".
- `adv2_a_url_that_is_none_is_refused_naming_database_url`: `postgres://` reads "no TLS connection to PostgreSQL at  verified against database_ca".

Red in debug only: the scan at 500k and at 1M events fails at 10.6 s with "event store deadline exceeded during store operation".

Measured: PostgreSQL scan, release, 100k 1.55 s 219 MiB, 200k 2.26 s 430 MiB, 500k 5.72 s 1062 MiB, 1M 7.99 s 2116 MiB; SQLite 100k 0.75 s 16 MiB, 200k 1.51 s 17 MiB, 500k 2.61 s 18 MiB.

Suite: kernel `--include-ignored --test-threads=1` 291 passed, 11 failed (the 7 new plus 4 pre-existing `adversary_look_ahead` cases); CLI `postgres` 12 passed.

Attacked and not broken: pass-1 findings 1 to 5; 118 scans during grouped writes, rows always 3 × version; two first scans at once; a role without INSERT on `_identity` is refused at open; no 60 s wait in `run`; encoded and `?password=` passwords are not printed; features sqlite-only, postgres-only and none compile; wasm32 has no PostgreSQL crate.

```findings
- file: crates/spindle-kernel/src/postgres.rs
  line: 224
  category: property
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a scan holds the component's whole log in memory in one 10 s transaction, about 2.1 KB per event (1062 MiB at 500k, 2116 MiB at 1M, against SQLite's flat 18 MiB), and fails every scan once the log outgrows the deadline (500k in debug, about 1.25M in release)
- file: crates/spindle-kernel/src/postgres.rs
  line: 172
  category: concurrency
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a scan holds eventlog's exclusive publication lock for its whole read while serving pools keep the 2 s lock_timeout, so commits and scans of a second process on the same tables fail with a deadline error whenever a scan runs over 2 s
- file: crates/spindle-kernel/src/postgres.rs
  line: 314
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the unreachable-server message prints the parsed host, which for a password with an unencoded @ holds the rest of the password (retpart@db.example.invalid:5432)
- file: crates/spindle-kernel/src/postgres.rs
  line: 247
  category: boundary
  severity: note
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a database_url with no host passes the parse check and with database_ca is reported as an unreachable server at an empty host instead of naming database_url
```
