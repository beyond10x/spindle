---
format: aep.planning-md/3
id: review-result:wave28-adversary-U5-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U5
relations:
- reviews: story:events-record-their-caller
- reviews: story:entity-event-history
revision: 1
---
unit: U5, story:events-record-their-caller + story:entity-event-history. Findings cover head 2af8e08b (implementation at 761b3f2a plus my tests only).
verdict: CONFIRMED
cases: executed 793→803, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 paths (part 6)
needs-coordinator: none

Two of my cases fail; both defects are in code this unit added. The four reported choices hold against the stories, each with a judgement note below. No PostgreSQL case ran (`SPINDLE_TEST_DATABASE_URL` not set). I did not run the IndexedDB task; I compared the browser store with SQLite by reading the code.

**1. `git --no-pager diff --stat 761b3f2a..HEAD`** (every path is a test file)
```
 .../tests/spindle-host/adversary_w28_u5.rs         | 284 +++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 .../tests/spindle-ingress/adversary_w28_u5.rs      | 195 +++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
 .../tests/spindle-kernel/adversary_w28_u5.rs       | 217 +++++
 crates/spindle-kernel/tests/spindle-kernel/main.rs |   2 +
 6 files changed, 701 insertions(+)
```
Committed as 2af8e08b on `unit/w28-history`.

**2. Cases added.** Each red case was run alone before any suite run.

`crates/spindle-host/tests/spindle-host/adversary_w28_u5.rs`
- `a_revision_after_a_restart_on_a_fixed_clock_is_read_after_an_earlier_cursor`: **RED.** On SQLite with `ManualClock::at(fixed)`, the test takes a cursor, restarts the host on the same clock and sends a revision. The cursor's next page should hold that revision. Captured when run alone:
  ```
  panicked at crates/spindle-host/tests/spindle-host/adversary_w28_u5.rs:279:5:
  the revision after the restart is read after the earlier cursor: History { events: [], next: HistoryCursor { run: 11443869869667797836, generation: 0, position: 15, sequence: 1800000000000003 } }
    left: []
   right: ["q5"]
  test result: FAILED. 0 passed; 1 failed; ... 209 filtered out
  ```
- `the_history_cursor_walks_one_write_at_a_time`: green. Limit 1 walks q1, q2, q4; the same cursor read twice gives the same page; the cursor stays put at the end; limit equal to the length gives the whole history, then an empty page.
- `an_unknown_entity_is_refused_and_another_component_reads_its_own`: green. Covers `NoEntity`, an identity of the wrong JSON type (empty), and the archive's `Copy` history whose binding cause carries the request id.
- `the_history_reads_alike_on_sqlite_and_memory`: green. Command, outcome, cause and event match on both stores, the `affects:` pair included.

`crates/spindle-ingress/tests/spindle-ingress/adversary_w28_u5.rs`
- `a_string_identity_of_digits_reads_its_history`: **RED.** A record with String identity `"42"` reads as `identity=%2242%22`, but `identity=42` answers an empty list. Captured when run alone:
  ```
  panicked at crates/spindle-ingress/tests/spindle-ingress/adversary_w28_u5.rs:148:5:
  the identity as the route carried it reads the record's history: {"events":[],"next":"WzEsMTYw..."}
    left: []
   right: [ {... "payload": {"record_id": "42", "text": "one"} ...}, {... "text": "two" ...} ]
  test result: FAILED. 0 passed; 1 failed; ... 155 filtered out
  ```
- `a_quoted_and_a_bare_text_identity_read_alike`: green.
- `a_bound_command_records_the_request_id_of_its_http_request`: green. Each HTTP request gets its own id, and the `keep-copy` command records the id of the HTTP request that triggered it.

`crates/spindle-kernel/tests/spindle-kernel/adversary_w28_u5.rs` (`cfg(feature = "sqlite")`). These pass and close gaps in the unit's suite:
- `the_append_records_the_actor_of_its_cause`: reads the event log directly. The append actor is the caller's actor for a request, a seed entry, and a binding whose trigger names a caller; it is `spindle-kernel` for a tick, an unknown trigger, and no origin. All 6 request ids differ, and none is the caller's `r-1`. No existing test asserted this append actor, so deleting `meta.actor = actor` (`sqlite.rs:819`) left the suite green.
- `the_written_origin_names_its_cause_as_documented`: the stored `origin.cause` JSON matches the `Provenance` doc literally, for both request and binding.
- `a_chain_of_bindings_records_the_request_that_started_it`: a two-hop binding chain keeps the first request as its trigger; an unknown trigger stays unknown.

All new tests run well under 5 s (0.00–0.05 s alone).

**3. Suite runs** (after the cases existed)
- `cargo test --locked -p spindle-host --no-fail-fast`: exit 101. Lib 1 passed; integration `FAILED. 195 passed; 1 failed; 14 ignored`, 10.33 s.
- `cargo test --locked -p spindle-ingress --no-fail-fast`: exit 101. Lib 21 passed; integration `FAILED. 155 passed; 1 failed; 0 ignored`, 10.63 s.
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 0. Lib 14 passed; integration `ok. 415 passed; 0 failed; 55 ignored`, 6.71 s.
- `cargo clippy --locked -p spindle-kernel -p spindle-host -p spindle-ingress --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-kernel -p spindle-host -p spindle-ingress -- --check`: exit 0.

`executed 793` is the after-count with my 10 cases taken out: host 193, ingress 174, kernel 426.

**4. Findings**

| # | file:line | finding | verdict | origin | what reaches it |
|---|---|---|---|---|---|
| F1 | `crates/spindle-ingress/src/console.rs:328` | Any identity text that parses as JSON is read as JSON, without the entity's declared identity type. So a String id of digits (also `true`, `null`) reads an empty history unless the consumer quotes it. The command route carries the same id unquoted. Fix: type the identity by the entity's identity field from the IR. | NEEDS-CHANGE | introduced | Any consumer whose record ids are digit strings and who calls `GET /api/history?identity=<id>`. |
| F2 | `crates/spindle-host/src/host/range.rs:518,538` | Across a restart, the cursor skips writes whose command sequence is ≤ the cursor's. The host's sequence starts again at 0 (`host.rs:1213`), and `following` gives the clock's time again. On a fixed civil clock, commands after the restart reuse the old sequences, and the earlier cursor skips them without any error. | CONFIRMED | introduced | `spindle serve --clock <at>` with `--state` on SQLite, restarted (`serve.rs:401-404`). A clock stepped back, e.g. by NTP, has the same effect. `/api/events`' `Cursor` uses the same sequence design. |
| F3 | `crates/spindle-cli/src/seed.rs:723` | Choice 1: `Provenance::Seed` records `kind: seed`, the actor, the caller and `request_id`, but not which seed. `Seed` has `path` and `digest`. Whether `kind: seed` satisfies "names the seed" is a reading question. The digest would name the seed without a host path. | CONFIRMED | introduced | Every seeded run. |
| F4 | `crates/spindle-kernel/src/store.rs:153,237,243` | `Origin`, which consumers build literally, gains a public field `cause`, and `Committed` gains `outcome` and `instances`. That breaks a consumer's own `Store` or test store. The CHANGELOG lists this under Added only. It is also why choice 4 had to edit two ingress tests outside the assignment (`cause: None`, mechanical and harmless). | CONFIRMED | introduced | Any crate that builds `Origin` or `Committed`; this repository needed 5 lines in 3 test files. |
| F5 | `crates/spindle-ingress/src/answer.rs:496` | The minted request id is not answered to the client and not kept in the request log. It groups the events of one request, but neither the caller nor an auditor can link it to an HTTP exchange. | CONFIRMED | introduced | Every HTTP command. |

**5. Attacked and not broken**
- Choice 2: binding records the root cause only; split run records `trigger: None`. Matches the acceptance and the D2 A decision. Multi-hop chains keep the root (kernel test).
- Choice 3: writes stored before this release name no instances. Matches D3 A and the `a_store_written_before_...` acceptance; the CHANGELOG says so.
- Paging: limit 1, limit equal to the length, a cursor at the end, a repeated cursor, writes larger than the page (`whole_writes`). `limit=0` is a 400.
- Unknown entity: host gives `NoEntity`, the route 400 without echoing the value. Unknown identity reads empty.
- Request id: one per HTTP request, carried through an in-process binding. The log column `request_id` stays fresh per append.
- Request-log elision: only `after` and `limit` are kept; `after` only on a 200 and only in cursor characters. Entity and identity are elided, including under percent-encoded keys.
- Browser store against SQLite (code read): the same `instances_of` order (entity, identity text), `commit` and `remove` go through `commit_all`, `commit_events` has no instances, and `outcome` resets on `attribute` in all stores.
- Outcome naming: every commit path in `execute.rs` goes through `take`, and `attribute(None)` after the command resets it.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>`
- `<local path>` (the redirect target of the first red run)

**7.**
```findings
- file: crates/spindle-ingress/src/console.rs
  line: 328
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: GET /api/history reads identity=42 as the JSON number 42 regardless of the entity's String identity type, so a digit-string record answers an empty history unless quoted
- file: crates/spindle-host/src/host/range.rs
  line: 538
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a history cursor taken before a restart skips revisions made after it when the host runs on a fixed civil clock, because the restarted host reuses the same command sequences
- file: crates/spindle-cli/src/seed.rs
  line: 723
  category: acceptance
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a seed event records kind seed, actor, caller and request_id but not which seed (no digest), a narrow reading of "names the seed"
- file: crates/spindle-kernel/src/store.rs
  line: 153
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: new public fields on Origin and Committed break consumers that build them literally, and the CHANGELOG lists the change only under Added
- file: crates/spindle-ingress/src/answer.rs
  line: 496
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the minted request id is neither answered to the client nor kept in the request log, so it cannot be linked to an HTTP exchange
```
