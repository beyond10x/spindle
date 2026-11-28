---
format: aep.planning-md/3
id: story:listener-limits
kind: story
status: active
title: The ingress listener bounds connections, headers and request time
summary: A connection cap, a head size limit and a per-request time, and a proxy-secret compare that does not reveal the length.
relations:
- serves: vision:spindle
- decomposes: epic:production-service
scope:
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/main.rs
- confidence: inferred
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-control/src/server.rs
- confidence: inferred
  path: crates/spindle-ingress/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/limits.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: website/docs/concepts/capabilities.md
- confidence: inferred
  path: website/docs/reference/cli.md
revision: 15
transitions:
- {from: "draft", to: "proposed", at: "2026-11-28T09:24:51Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":7}}}
- {from: "proposed", to: "active", at: "2026-11-28T09:24:51Z", actor: "human:operator", revision: 14, decided_on: {"recorded":{"review_outcome":7}}}
---
# The ingress listener bounds connections, headers and request time

## Outcome

The listener holds at most a configured number of connections (512 by default; one over the cap
waits in the kernel's backlog until a connection closes), answers a head over a configured size
(16 KiB) or header count (100) with `431`, closes a keep-alive connection that stays idle past the
head read time so idle connections cannot hold the cap, and answers every request within a
configured time (30 s, counted from the end of its body read): `503` with `committed: false` for a
request no thread has taken yet, which is then never handed, and `504` for one already taken, whose
command still completes. `run --max-connections`, `--max-head-bytes` and
`--request-timeout <SECONDS>` set them, through `Surface::limited(Limits)`. The proxy secret, and
the token `spindle-control` checks, are compared in time that does not reveal their length.

## Why

Line numbers at 276298bb.

- Each accepted connection is spawned with no cap (`crates/spindle-ingress/src/listener.rs:573-580`,
  in `Listener::serve_over` `:567-585`).
- The HTTP/1.1 builder sets only a header read timeout (`HEADER_READ`, 10 s, `listener.rs:39`;
  builder `:707-710`): no limit on header count or head size beyond hyper 1.12.0's defaults (100
  headers; no head size limit, so its read buffer of about 400 KB applies).
- The body read is bounded (`BODY_READ`, `BODY_LIMIT`, `listener.rs:902-906`), but a request then
  waits for the host's answer with no bound (`handed`, `listener.rs:1006-1012`), so one slow
  command holds every request queued behind it.
- `same` (`listener.rs:303-310`), which `presents` uses for the proxy secret (`:959-963`) and
  `Guard::refuses` (`:265`, `:270`) and `Guard::strip` (`:283`) use for the token, returns at once
  when the lengths differ, so a caller learns the secret's length. `spindle-control` has a copy
  with the same leak (`crates/spindle-control/src/server.rs:165-172`).

## Acceptance

Each test below runs in the default test step, on loopback only, in
`crates/spindle-ingress/tests/spindle-ingress/limits.rs` unless it names another place. Each sets its
bounds small through `Limits` and ends within 3 s; none waits on `HEADER_READ` (10 s).

- `a_connection_over_the_cap_waits_until_one_closes` (was `connections_over_the_cap_are_refused`;
  D2 A): with a cap of 2, two open connections are answered; a third gets no answer within 500 ms
  while both stay open, and is answered within 2 s once one of them closes.
- `a_head_over_the_limit_answers_431`: a head over the size limit, and one with more headers than
  the header limit, each answer `431` and the connection closes. The `431` may carry an empty body:
  hyper writes it before the service runs. "Writes nothing" means no exchange reaches
  `Listener::next` and `GET /__spindle/requests` holds no record of the request. hyper 1.12.0's
  `http1::Builder::max_header_size` (start line and headers) and `max_headers` set the two limits;
  `max_buf_size`, which panics below 8192, is not needed for them.
- `an_idle_connection_is_closed_and_frees_its_place`: with a cap of 1 and the head read time set
  to 1 s through `Limits` (default `HEADER_READ`, 10 s), a keep-alive connection that was answered
  once and then sends nothing is closed by the listener within 2 s, and a second connection waiting
  at the cap is then answered. The bound is the head read time, so an idle connection holds a place
  of the cap for at most that long. hyper 1.12.0 starts its `header_read_timeout` when it polls for
  the next head of a kept-alive connection (`src/proto/h1/conn.rs:224-271`,
  `src/proto/h1/dispatch.rs:229`; read, not run); where it does not close the connection, the
  listener closes it itself within the same bound.
- `a_request_past_its_time_answers_and_frees_the_listener` (D3 A, D4 A): with a request time of
  1 s, a command taken by `Listener::next` and not answered within 1 s of the end of its body read
  answers `504` to its caller; the thread's later answer to it is dropped without error, and the
  next request is answered.
- `a_queued_request_past_its_time_answers_503_and_is_never_handed` (D3 A): with a request time of
  1 s and no call to `Listener::next`, a command answers `503` with `{"refused", "committed":
  false}` after 1 s, and a later `Listener::next` does not hand it.
- `the_secret_compare_takes_the_same_time_for_any_length` (unit test in `listener.rs`, a test-only
  byte counter): the compare reads the same number of bytes for a presented value shorter, longer
  and equal in length to the secret.
- `the_control_token_compare_takes_the_same_time_for_any_length` (unit test in
  `crates/spindle-control/src/server.rs`, D5 A): the same for the copy of `same` there.
- `run_flags_bound_the_listener` (a new module of `crates/spindle-cli/tests/spindle-cli/`, default
  step, under 5 s): one served `run --max-connections 1 --max-head-bytes 8192 --request-timeout 1`
  shows each flag taking effect, each check one the defaults would not pass:
  - a head of 12 KiB (over the flag's 8192, under the 16 KiB default) answers `431`;
  - while a first connection stays open, a second gets no answer within 500 ms, and is answered
    within 2 s once the first closes;
  - a command held on a loopback desk that answers only when the test releases it (the `Desk` of
    `concurrent_reads.rs:62`) answers `504` within 2 s of being sent; the desk is released after.

  A `run` that parses the three flags and does not pass them to `Surface::limited` fails each of
  the three checks. `run --help` names the three flags with their defaults.
- No new test: the default request time (30 s) applies to every existing test that holds a
  request. The existing PostgreSQL-lane tests (`task check:postgres`) and the default-step tests of
  `spindle-ingress`, `spindle-control` and `spindle-cli` pass unchanged with it; none waits on a
  held request over 30 s (the longest such wait read in the lane is 11 s,
  `crates/spindle-cli/tests/spindle-cli/adversary_w31_u6.rs:1888`). The unit's report names the lane
  run and the step run that show it.
- `website/docs/reference/cli.md` is regenerated from `crates/spindle-cli/src/cli.rs` and the docs
  check stays green; `website/docs/concepts/capabilities.md`'s status table (`:228-237`) lists
  `431`, and the `503` and `504` of a request past its time; `AGENTS.md:38`, the `spindle-ingress`
  row, names `Limits`; the change goes under `[Unreleased]` in `CHANGELOG.md`.

## Decided when scoped

Taken 2026-11-28 on the operator's pick (D-W33b A): the scoper's default, option A, of every
decision, each reversible. No new dependency.

- D1 Defaults: A, 512 connections, 16 KiB head, 100 headers, 30 s per request.
  Not taken: B (1024 connections, 64 KiB head, 60 s), C (no connection cap by default).
- D2 At the cap: A, the listener stops accepting until a connection closes; the kernel's backlog
  holds the rest.
  Not taken: B (accept, answer `503`, close), C (accept and close).
- D3 A command past its time: A, a claim decides: a request still queued answers `503` with
  `committed: false` and `Listener::next` skips it; one already taken answers `504` and its command
  completes. The `503` is built in `listener.rs` from the existing `Answer::refusal`, as
  `Listener::close` builds its `503` (`listener.rs:656`), with `"committed": false` added to its
  body: `Answer::refusal` writes `{"refused": <detail>}` only (`answer.rs:42-48`) and `Answer`'s
  fields are public (`answer.rs:27-37`). This story does not edit
  `crates/spindle-ingress/src/answer.rs`; in wave 33 that file is
  `story:new-leader-write-while-old-paused`'s (its `503` for a store deadline).
  Not taken: B (always applied, `504`).
- D4 Where the request time starts: A, at the end of the body read; `BODY_READ` keeps its 10 s.
  Not taken: B (from the head).
- D5 The copy of `same` in `crates/spindle-control/src/server.rs:165`: A, fixed in this story.
  Not taken: B (a story of its own).
- D6 How the bounds are set: A, `Surface::limited(Limits)` beside `requiring_secret`, plus
  `run --max-connections`, `--max-head-bytes` and `--request-timeout <SECONDS>`; none of the 159
  `Listener::bind*` call sites changes. `Limits` holds the four bounds of D1 (connections, head
  size, header count, request time) and one more field, the head read time (default
  `HEADER_READ`, 10 s), which bounds how long an idle keep-alive connection holds a place of the
  cap and which `an_idle_connection_is_closed_and_frees_its_place` sets to 1 s. `run` has no flag
  for the header count or the head read time.
  Not taken: B (new `bind_*_limited` constructors).

## Scope

Derived 2026-11-28 by `story-scoper` on `plan/wave-33` (head 276298bb). Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Stale claims:** every line number in the story is stale; each claim still holds — cited. Now: spawn with no cap `listener.rs:573-580`; `HEADER_READ` `:39`; builder `:707-710`; body read `:902-906`; unbounded wait `handed` `:1006-1012`; `same` `:303-310` (also used by `Guard::refuses` `:265`, `:270`, `Guard::strip` `:283`); `presents` `:959-963`.
- **Primary surface:** `crates/spindle-ingress/src/listener.rs` — cited
- **Files:** `listener.rs`: accept loop `Listener::serve_over` `:567-585`; `answering` `:707-710`; `handed` `:1006-1012`; `same` `:303-310`; `Listener::next` `:616-623` — cited
- **Files:** the queued request's `503` `{"refused", "committed": false}` is built in `listener.rs` from `Answer::refusal` (as `Listener::close` does at `:656`) with `"committed": false` added to the body; `Answer`'s fields are public (`crates/spindle-ingress/src/answer.rs:27-37`). `answer.rs` is not edited here: in wave 33 it is owned by `story:new-leader-write-while-old-paused` (D3 A) — cited
- **Also likely:** a public `Limits` type re-exported from `crates/spindle-ingress/src/lib.rs:98`, holding the connection cap, head size, header count, request time and the head read time (default `HEADER_READ`, 10 s; used by `an_idle_connection_is_closed_and_frees_its_place`, D6 A); `Surface::limited(Limits)` in `crates/spindle-ingress/src/surface.rs` beside `requiring_secret` `:1518`, so none of the 159 `Listener::bind*` call sites changes — inferred
- **Also likely:** `crates/spindle-cli/src/cli.rs` (flags beside `--drain` `:575-594`), `crates/spindle-cli/src/main.rs` (`:240-290`, `:654-690`), `crates/spindle-cli/src/serve.rs` (`Serving` `:80-160`, surface built before `bind_reading_on`/`_tls`/`bind_standing_by_on` `:526`, `:621-630`) — inferred
- **Also likely:** `crates/spindle-control/src/server.rs:165-172`, a copy of `same` with the same length leak (D5 A) — inferred
- **Tests** (loopback only, default test step): in `crates/spindle-ingress/tests/spindle-ingress/limits.rs` (new, each under 3 s): `a_connection_over_the_cap_waits_until_one_closes`, `a_head_over_the_limit_answers_431`, `an_idle_connection_is_closed_and_frees_its_place`, `a_request_past_its_time_answers_and_frees_the_listener`, `a_queued_request_past_its_time_answers_503_and_is_never_handed`; unit tests `the_secret_compare_takes_the_same_time_for_any_length` in `listener.rs` and `the_control_token_compare_takes_the_same_time_for_any_length` in `crates/spindle-control/src/server.rs` (test-only byte counter); `run_flags_bound_the_listener` in a new module of `crates/spindle-cli/tests/spindle-cli/` (under 5 s). None waits on `HEADER_READ` (10 s).
- **Documents:** `website/docs/reference/cli.md` is generated from clap (`crates/spindle-docs/src/cli.rs:10`); `CHANGELOG.md`; the `spindle-ingress` row of `AGENTS.md:38` — inferred
- **Would collide with:** units on `listener.rs`, `surface.rs` builders, the `run` flag set (`cli.rs`, `main.rs`, `serve.rs` `Serving`), `CHANGELOG.md`, `AGENTS.md`
- **Safety fact:** every limit applies inside `Listener::serve_over` and `answering`; all `bind*` callers reach them only through `serve_over` (`listener.rs:504-520`) — step 2, unproven
- **Safety fact:** hyper 1.12.0 answers a head over `max_header_size` (start line and headers) or `max_headers` with `431` and closes, before the service runs; `max_header_size` has no lower bound, so the 8192 floor of `max_buf_size` (which panics below it) does not apply — step 1, unproven
- **Checked when planned (2026-11-28, read against 276298bb):** every line above holds. Added:
  `website/docs/concepts/capabilities.md` (its status table, `:228-237`, gains `431` and the
  `503`/`504` of a request past its time; U2 of wave 33 edits the same page) — inferred. hyper
  1.12.0 also has `http1::Builder::max_header_size` (start line and headers, `431` and close, no
  lower bound), which sets the head limit without `max_buf_size`'s 8192 floor — cited
  (`src/server/conn/http1.rs:346-358`, `:399-404`)
- **Revised after plan-critic round 1 (2026-11-28, read against fc19c7d9):** the Tests line names
  the Acceptance tests; the head limit is `max_header_size`; `Limits` carries the head read time;
  `answer.rs` stays U2's — cited

## Delivered

Wave 33, unit U1, merged into wave/33 at 3d8908b9; adversary rounds 1-4, round 4 refuted.

Changed from the plan, by the coordinator during the wave:

- The head size default is 64 KiB (65536), not 16 KiB: 16 KiB refused two existing cases (a
  60,000-byte path, a view-list query of about 30 KiB). `a_head_under_the_default_is_answered` holds it.
- A client that does not read its answer holds its place for at most `Limits::head_read`: the
  socket has a write deadline below TLS (round 1).
- A command past its request time is never handed, also when its client left: the deadline is kept
  in the exchange (round 2).
- On close, connections waiting at the cap are accepted and answered as during the drain: `503`
  outside `/__spindle/`, the probes there as always (rounds 2 and 3).
- A body is refused `408` when it pauses longer than the head read time between two pieces, or
  takes over 10 s in all; a body arriving steadily under both is handed (round 3). At a head read
  time of 10 s the answer is always the whole-body text (round 4).
