---
format: aep.planning-md/3
id: story:refused-bound-command-recorded
kind: story
status: draft
title: A refused bound command is recorded where a run can read it
relations:
- informed_by: story:stdin-run-fires-bindings
- serves: vision:spindle
scope:
- confidence: cited
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: README.md
- confidence: inferred
  path: crates/spindle-channel/src/endpoint.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/refusal.rs
- confidence: cited
  path: crates/spindle-ingress/src/answer.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe.rs
- confidence: cited
  path: crates/spindle-ingress/src/observe/metrics.rs
- confidence: cited
  path: crates/spindle-ingress/src/router.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/fixtures/refused-bound
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/observe.rs
- confidence: cited
  path: crates/spindle-observe-api
- confidence: cited
  path: docs/design/spindle.md
- confidence: cited
  path: ess/domains/observe.yaml
- confidence: cited
  path: ess/fixtures/observe/metrics/families.json
- confidence: cited
  path: ess/fixtures/observe/requests
- confidence: cited
  path: ess/generated/jsonschema
- confidence: cited
  path: ui/src/api/generated
- confidence: cited
  path: ui/src/api/shapes.ts
- confidence: cited
  path: ui/src/api/types.ts
- confidence: cited
  path: website/docs/concepts/observe.md
revision: 29
---
# A refused bound command is recorded where a run can read it

## Outcome

When a binding's command is refused (a declared refusing outcome, or a kernel rejection), a served run
records it: the bound command, the binding, the refusal's outcome or error code, and the command
that caused it. Today the host reports a failed invocation once on stderr and records nothing a run
can read, so a consumer cannot count or show refused reactions. A request record of
`GET /__spindle/requests` already lists every binding whose input was unmet on the way (`unmet`,
`crates/spindle-ingress/src/observe.rs`); a refused bound command has no such entry.

Where the record lives is decided when scoped:

- A: in the request record of the request that caused it, beside `unmet` (`refused_bound`), and as a
  count in `GET /__spindle/metrics`; the event log stays the events ESS commits.
- B: an entry in the event log (`GET /api/events`), which ESS's interpreter does not write.

## Acceptance

- `a_refused_bound_command_is_listed_with_its_cause`: a binding from `E` to `C` where `C` refuses
  with a declared outcome; the request record of `GET /__spindle/requests` lists one `refused_bound`
  entry naming `C`, the binding, the outcome (and its declared error, if any) and the command that
  published `E`.
- `a_bound_command_that_runs_adds_no_refusal_entry`: the same binding where `C` runs; the record's
  `refused_bound` is `[]`.
- `a_refused_bound_command_records_no_error_field_value`: the entry names the outcome and error, and
  holds none of the error's field values.
- `a_refused_bound_command_is_counted_in_the_metrics`: after the refusal,
  `GET /__spindle/metrics` reads `spindle_bound_refusals_total{binding, command, refusal}` 1 (D2 A).
- `a_rejected_bound_command_is_listed_with_its_kind`: the same binding where the kernel rejects
  `C` (no declared outcome, `Failed`); the entry names `C`, the binding, the kernel's error kind
  and the causing command.
- `a_refusal_from_another_process_has_no_cause`: in `crates/spindle-host`, a `RefusedCommand` for
  an envelope delivered from another process (a split run's channel) carries `caused_by: None`.
- `a_refusal_without_a_cause_has_no_cause_member`: in `crates/spindle-ingress`, a request record
  built from a `RefusedCommand` with `caused_by: None` lists its `refused_bound` entry with no
  `caused_by` member.
- `the_request_log_stays_version_1`: `GET /__spindle/requests` with a refusal entry answers
  `"format": "spindle-requests/1"` (D3 A).

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0 over HTTP: a reaction refused when a balance is unchanged
left no trace a run could count.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress`, the request record and the dispatch that feeds it — cited
- **Files:** `crates/spindle-ingress/src/answer.rs:486-509` (`run` returns `report.failed` only; `report.refused` is dropped at `:508`), `:534-550` (Runaway path) — cited
- **Files:** `crates/spindle-ingress/src/observe.rs:286-322` (`UnmetInput`, the pattern for the new entry), `:324-376` (`Answered` gains `refused_bound`), `:409-478` (`Record` and its JSON), `:609-670` (`Recorder::record` copies `answered`), `:494`/`:728` (the externals counter, the pattern for D2 A) — cited
- **Files:** `crates/spindle-ingress/src/router.rs:359-374` (`Pending::serve` fills `unmet` from `failed`; kernel rejections, `attempts >= 1`, come from the same list) — cited
- **Files:** `crates/spindle-ingress/src/observe/metrics.rs:47-160` (`FAMILIES: [Family; 16]`, held equal to `families.json` by a unit test), `:388`, `:422` — cited
- **Files:** `crates/spindle-host/src/host.rs:77-107` (`Failed`, `RefusedCommand`: add `caused_by`, and on `Failed` the kernel kind from `ExecuteError::kind`, `spindle-kernel/src/lib.rs:239`), `:1764-1838` (`execute`, where events are published), `:1859-1872` (`invoke`), `:1876-1944` (`attempt`), `:1946-1962` (`unmet`) — cited sites, change inferred (D1 A)
- **Files:** `crates/spindle-channel/src/endpoint.rs:122-182` (`Sender::publish`; an additive read-only accessor for the next `DeliveryId`; `Envelope` is not changed, it is a TCP wire frame) — inferred (D1 A)
- **Specification:** `ess/domains/observe.yaml:355-445` (`RequestRecord` gains `refused_bound: List<spindle.observe.RefusedBound>`), new struct beside `UnmetInput` at `:779`: binding, component, command, caused_by, outcome and declared error name, or kernel kind, attempts; no error field values — cited place, inferred shape
- **Generated (with `task generate:observe`, `Taskfile.yml:244-261`; never edited by hand):** `ess/generated/jsonschema/schema/types/spindle.observe.RequestRecord.schema.json`, `…RequestLog.schema.json`, new `…RefusedBound.schema.json`, `ess/generated/jsonschema/.ess-output/state.json`, `crates/spindle-observe-api/{types.rs,source.schema.json,types-report.json}`, `ui/src/api/generated/{types.ts,source.schema.json,types-report.json}` — cited
- **Fixtures:** `ess/fixtures/observe/requests/{scripted,unmet,externals,declared-5xx}.json`: every record gains `"refused_bound": []` (a List is required, `RequestRecord.schema.json:167-178`); `scripted.json` is compared with a live log (`crates/spindle-cli/tests/spindle-cli/observe.rs:37`) — cited. `ess/fixtures/observe/metrics/families.json`: one new family — cited
- **UI:** `ui/src/api/shapes.ts:216-222` (conform refuses an undeclared field, `ui/src/api/types.test.ts:52-53`), `ui/src/api/types.ts:56` (exports) — cited
- **Tests:** in `crates/spindle-ingress/tests/spindle-ingress/observe.rs`, next to `:292`: `a_refused_bound_command_is_listed_with_its_cause`, `a_bound_command_that_runs_adds_no_refusal_entry`, `a_refused_bound_command_records_no_error_field_value`, `a_refused_bound_command_is_counted_in_the_metrics`; no new module, so `main.rs` is untouched — inferred
- **Tests:** new fixture `crates/spindle-ingress/tests/fixtures/refused-bound/` (binding `E` → `C`, `C` with a declared refusing outcome chosen without an external decider, emitter `reached_by: network`). `library` has `errors: []` (`notices.yaml:19`), and the host's `refusal` fixture is `in_process` and decided externally — inferred
- **Tests:** `crates/spindle-host/tests/spindle-host/refusal.rs`: `a_delivered_refusal_names_the_command_that_published_its_event` — inferred (D1 A). Every test is in memory or on loopback, the same shape as the `unmet` test at `:292`; each should run well under 60 s, not measured
- **Documents:** `website/docs/concepts/observe.md:60` (metric table), `:76` (`unmet` paragraph); `README.md:91`; `AGENTS.md:38` (spindle-ingress row); `CHANGELOG.md`; `docs/design/spindle.md` § 11 (a row next to `:419`; ESS defines no observable for a refused bound command) — cited lines, inferred that each changes
- **Not touched:** `crates/spindle-cli/src/serve.rs:465-479` (the stderr loop keeps printing `failed` only); the event log (`GET /api/events`; option A is decided) — inferred
- **Confidence:** high for the ingress record (the story names `observe.rs`, and the drop at `answer.rs:508` was read); medium for host and channel (depends on D1)
- **Would collide with:** `run` in `crates/spindle-ingress/src/answer.rs`; `Failed`/`RefusedCommand`, `execute` and `attempt` in `crates/spindle-host/src/host.rs`; `ess/domains/observe.yaml` and its generated projections; `ui/src/api/shapes.ts`; `observe.rs`/`metrics.rs` of spindle-ingress; `CHANGELOG.md`, the crate table in `AGENTS.md`, `docs/design/spindle.md` § 11
- **Safety fact:** `Failed` and `RefusedCommand` are built only in `host.rs` (`:1916`, `:1931`, `:1952`; `git grep` finds no struct literal elsewhere), so new fields break no construction in another crate — level 2, unproven
- **Safety fact:** the generated `RequestRecord` is `#[serde(deny_unknown_fields)]` (`crates/spindle-observe-api/types.rs:565`), so a reader on an older spindle-observe-api refuses a record that has `refused_bound`. Earlier, `unmet` and `externals` were added under the same `spindle-requests/1` — level 2, unproven

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- Place, A: the request record of `GET /__spindle/requests`, a list `refused_bound` beside `unmet`,
  plus a metrics count; the event log stays what ESS commits. The consumer confirmed it reads that
  record per step; the host operator confirmed it on 2026-11-26.
- D1 A: the host sets `caused_by` on `RefusedCommand` and `Failed` from the command whose publish
  staged the envelope, through a read-only `Sender` accessor in `spindle-channel`; empty for an
  envelope from another process.
- D2 A: the Recorder counts each record's list as
  `spindle_bound_refusals_total{binding, command, refusal}`, as it counts externals.
- D3 A: the request log keeps `spindle-requests/1`, as when `unmet` and `externals` were added.
- Not covered: a refusal inside a split run's other process, a refused periodic-binding command (no
  request), refusals before a runaway (`HostError::Runaway` carries only `failed`).
