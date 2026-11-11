---
format: aep.planning-md/3
id: review-result:w19-u2-adversary-1
kind: review-result
status: active
title: Wave 19 U2 adversary pass 1
relations:
- reviews: story:http-caller-proves-actor
revision: 1
---
needs-revision

```
unit: wave 19 U2 story:http-caller-proves-actor, commit 79be6ef0 on 43daf083 (uncommitted test files only)
verdict: NEEDS-CHANGE
cases: executed spindle-cli 393→399, red 3
origin: introduced 2 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_trusted_caller.rs`:

- `an_unproven_reset_writes_no_row_on_a_run_that_proves_callers`: red. `POST /api/reset` with a world body and no proof answers 200 and plants a Visit for any `person_id`.
- `an_unproven_request_reads_no_caller_s_events`: red. `GET /api/events` with no proof answers every caller's event payloads, around the read grant on `VisitLog`.
- `a_proof_naming_actor_twice_proves_no_caller`: red. `{"actor":Auditor,"actor":Member}` runs as Member (serde_json keeps the last key).
- `header_shapes_that_prove_no_caller_write_nothing`, `header_names_match_in_any_case`, `a_kept_alive_connection_carries_no_proof_from_one_request_to_the_next`: green.

Suite: `cargo test -p spindle-cli -p spindle-ingress`: `FAILED. 345 passed; 6 failed; 5 ignored` (3 red cases, 3 known ui/dist failures).

Findings:

- `crates/spindle-cli/src/main.rs:485`, `crates/spindle-ingress/src/surface.rs:949`: on a run that proves callers, the console (`/api/*`) and the reserved routes (`/__spindle/*` with `--observe`) on the same listener read no proof, so a request with none resets and seeds the stores and reads every caller's events. Design § 11 says they do not read the header and tells no one to block them. NEEDS-CHANGE, introduced, blocker.
- `crates/spindle-ingress/src/surface.rs:131`: `Caller::proven` accepts duplicate keys and keeps the last. INFEASIBLE through known proxies, introduced, note.

Attacked and not broken: header case variants, repeats, comma-joined, empty, folded, non-UTF-8, prefix and extended secrets; malformed, undeclared or mistyped proofs; 200 kB values; HTTP/1.0; keep-alive; a proven actor without the grant; the transport-free router; the header without the option; timing leaks length only; no secret in Debug, logs, the observe record or messages.

```findings
- file: crates/spindle-cli/src/main.rs
  line: 485
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: on a run with --trusted-caller-header the unguarded console on the same listener lets a request with no proof wipe and seed the stores through POST /api/reset and read every caller's events through GET /api/events
- file: crates/spindle-ingress/src/surface.rs
  line: 131
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: Caller::proven accepts a proof with duplicate keys and keeps the last actor, so {"actor":Auditor,"actor":Member} runs as Member instead of being refused
```
