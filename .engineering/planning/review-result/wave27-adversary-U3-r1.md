---
format: aep.planning-md/3
id: review-result:wave27-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 27 U3
relations:
- reviews: story:query-input-beside-body
- reviews: story:caller-attributes-on-loopback
revision: 1
---
```
unit: story:query-input-beside-body, story:caller-attributes-on-loopback, head 0efe86154 (adversary tests committed on top as d1f66c443)
verdict: CONFIRMED
cases: executed 219→238, red 4
origin: introduced 2 / pre-existing 1 / undecided 0
wrote-outside-worktree: 1 directory (scratch, below)
needs-coordinator: none
```

**1. `git --no-pager diff --stat 0efe86154..d1f66c443`.** Only test files and test fixtures changed.
```
 .../adversary-w27-u3-callers/domains/visit.yaml    |  70 +++
 .../fixtures/adversary-w27-u3-callers/system.yaml  |  23 +
 .../tests/spindle-cli/adversary_w27_u3.rs          | 157 +++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
 .../tests/spindle-ingress/adversary_w27_u3.rs      | 680 +++++++++++++++++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
```

**Summary.** The query-string reading matches ESS 0.99.1 everywhere I tried, and so does the stderr hint. Two problems were found:
- **Bare body (`request.body: input.<field>`):** the ingress never reads one. It reads the JSON body as an object. This was already true at the base for a bare body beside the path. The unit makes it reachable on routes that were 501 before: a bare body beside `request.query`, a combination ESS-HTTP-020 allows.
- **Check order on a route with a path variable:** the query string is checked before the path. ESS checks the path first (`with_path`, then `flat_object`). Both answer 400; only the refusal text differs.

**2. Cases added** (red output captured when each case was first run on its own)

`<local path>` uses its own `board` model, routes compiled by ESS, and ESS's interpreter as the oracle. Four cases are red:
```
---- a_bare_body_beside_the_query_is_read_as_its_one_field (:391)
assertion `left == right` failed: board.pin.PinBare on {"at":"2026-11-26T10:00:00Z","count":3,"holder":"h1","label":"hello"}: {"refused":"the body is not a JSON object: \"hello\""}
  left: 400
 right: 200
---- an_object_body_on_a_bare_route_is_not_read_as_members (:445)
assertion `left == right` failed: ESS's server reads the object as the value of `label` and refuses it: {"outcome":"pinned","published":[{"event":"board.pin.Pinned","payload":{"at":"2026-11-26T10:00:00Z","count":3,"holder":"h1","label":"hello"}}]}
  left: 200
 right: 400
---- a_bare_body_beside_the_path_is_read_as_its_one_field (:406)   [route without request.query]
assertion `left == right` failed: board.pin.Tag on {"holder":"h1","label":"hello"}: {"refused":"the body is not a JSON object: \"hello\""}
  left: 400
 right: 200
---- the_path_join_is_refused_before_the_query_is_read (:601)
/held/h1?count=1&count=2 {...}: ESS refuses `body.holder: the path carries this field, so the request does not` first; Spindle refused "query.count: expected one value, found 2"
```
Ten green cases are kept as guards:
- every route is served
- form-decoding: `+` becomes a space, `%2B`, `%26`, `%3D`, a key written `%63ount`, `007`, `-12`
- an empty value and a key with no `=`
- typed refusals: `1.5`, `+5`, `x`, `1e3`, a bad escape in a value and in a key, a missing required field
- query and body instants checked against the interpreter
- a JSON route under `unknown: ignore`
- a body that is not an object
- `the_query_refusal_wins_over_a_body_naming_a_queried_field`: no implementor case covers this order. Not run against a mutant.
- `a_route_without_request_query_reads_no_query_string`: catches removing the early return in `beside`. Not run against a mutant.

`<local path>` adds 5 green cases, with the fixture `crates/spindle-cli/tests/fixtures/adversary-w27-u3-callers` (two commands that read the caller, one that doesn't). They show the hint is:
- printed once for two caller-reading routes
- absent with `--fixed-actor`/`--fixed-caller`
- absent with `--trusted-caller-header`/`--trusted-caller-secret`
- absent when no command reads the caller
- absent with `--address 0.0.0.0`, where the run exits before listening

These catch a hint printed for every route and a hint printed unconditionally. Neither mutant was run.

No new test takes more than 5 s. The ingress module ran in 0.01 s and the CLI module reported 0.02 s.

**3. Suite runs** (after the cases existed)
- `cargo test --locked -p spindle-ingress --no-fail-fast`: 20 + 135 cases, 4 failed (the four above), exit 101, 11.23 s.
  - The same with `--skip adversary_w27_u3` gives the "before" count: 20 + 121 passed, exit 0.
- `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast -- caller ingress adversary_w27_u3`: 83 passed, 1 ignored, exit 0.
  - With `--skip adversary_w27_u3`: 78 passed, exit 0.
- `cargo clippy --locked -p spindle-ingress -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-ingress -p spindle-cli -- --check`: exit 0.

**4. Findings** (cover head 0efe86154)

| file:line | verdict | origin | what was measured | what reaches it |
|---|---|---|---|---|
| `crates/spindle-ingress/src/answer.rs:216` | NEEDS-CHANGE | introduced | A bare-body route beside `request.query` refuses a JSON text body (400) and runs on an object body (200). ESS does the opposite: it runs on the text and refuses the object. | A binding with `request: {body: input.x, query: [...]}`, which ESS-HTTP-020 allows. It was 501 at the base; `served` (`surface.rs:1840`) now serves it. Suggested fix: leave a route with `request.body` not served, or read the body as that one field. |
| `crates/spindle-ingress/src/answer.rs:216` | CONFIRMED | pre-existing | The same defect for a bare body beside the path, on a route without `request.query`. | A binding with `request.body` and a path carrying every other field. The base code has no `body_field` read at all. |
| `crates/spindle-ingress/src/answer.rs:233` | CONFIRMED | introduced | `beside` runs before `run`'s path check (`:473`), so on a route with a path variable the query error or `body.<queried>` comes back where ESS answers `body.<path field>`. | A route with a path variable and `request.query`, and a request wrong in both places. Only the 400 text differs. |

These are judgement calls I did not write as tests:
- The implementor changed an existing assertion in `fixed_caller.rs:234` to a narrower substring. The intent ("no caller is fixed by default") still holds.
- The declared deviation on a JSON route under `unknown: ignore` stands: a member under a renamed field's own name is refused by Spindle and dropped by ESS. Spindle reads JSON bodies by field name, not wire name, at the base too.

**5. Attacked and not broken**
- Percent-encoding and `+` in keys and values.
- Repeated scalar keys; empty values; a key with no `=`.
- Integer, Boolean, Timestamp and Optional values, checked against the interpreter.
- Under `unknown: ignore`: unknown and body-field query keys are dropped.
- The query error wins over a body naming a queried field.
- Routes without `request.query` keep their bytes.
- A body that is not an object answers 400.
- The hint: one line, only when neither setting is given, and not printed off loopback.

**6. Path written outside the worktree**
- `<local path>`: run logs, `m.rs` (a scratch copy of the module) and `commit-msg.txt`.

```findings
- file: crates/spindle-ingress/src/answer.rs
  line: 216
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a route with a bare request.body beside request.query, 501 at the base, is now served and reads the body as an object, answering 400 for a JSON text body and running on an object body where ESS 0.99.1 does the opposite
- file: crates/spindle-ingress/src/answer.rs
  line: 216
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: the ingress never reads a bare request.body as its one field, so a bare-body route beside the path answers 400 to a JSON text body that ESS runs
- file: crates/spindle-ingress/src/answer.rs
  line: 233
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the query string is read before run's path check, so a body naming the path's field gets the query's 400 text where ESS answers body.<path field> first
```
