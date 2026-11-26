---
format: aep.planning-md/3
id: story:caller-attributes-on-loopback
kind: story
status: draft
title: A loopback served run serves a command that reads its caller
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/caller.rs
- confidence: cited
  path: crates/spindle-cli/tests/spindle-cli/ingress.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: inferred
  path: docs/design/spindle.md
revision: 10
---
# A loopback served run serves a command that reads its caller

## Outcome

A command that reads caller attributes is served on a loopback run where no caller is fixed. Today
the ingress names it not served, "this ingress does not serve the caller attributes it reads
(`<attribute>`), which no HTTP caller proves here" (`crates/spindle-ingress/src/surface.rs`, `served`),
unless the run fixes a caller (`--fixed-caller`) or is served behind a proxy that proves each caller
(`Surface::proving_callers`, the caller header with the proxy secret). The story decides, when
scoped, which of these holds and delivers it:

- each request on a loopback run may prove its caller in the caller header, without a proxy secret,
  because only local processes reach the listener; or
- the existing routes already serve it, and the refusal names the flag or setting that does.

## Acceptance

- `a_loopback_request_proving_its_caller_runs_the_command`: a command guarded on a caller attribute
  answers as ESS's interpreter answers for that caller; or, for the second branch,
  `the_refusal_names_how_to_serve_a_caller_reading_command`.
- `an_exposed_run_still_requires_the_proxy_secret`: unchanged.

## Origin

A consumer, 2026-11-26, on Spindle 0.21.0: a served run of its specification reported this reason for
`POST /api/v1/kv` (attribute `account_id`); no journey reached the route.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Branch settled:** the second. A loopback run already serves a command that reads its caller
  with `--trusted-caller-header <NAME>` and `--trusted-caller-secret <FILE>`
  (`crates/spindle-cli/src/main.rs:502-531` refuses the pair only off loopback;
  `crates/spindle-cli/src/serve.rs:323-330` builds `Surface::proving_callers`;
  `crates/spindle-ingress/src/surface.rs:1753` skips the refusal where `route.proves`), or with
  `--fixed-actor`/`--fixed-caller` (`Surface::fixing_callers`, `route.caller` set). The test
  `crates/spindle-cli/tests/spindle-cli/trusted_caller.rs:263` already runs this on loopback (no
  `--address`) — cited
- **Primary surface:** `crates/spindle-ingress` reason text, plus `crates/spindle-cli` startup line — cited
- **Files:** `crates/spindle-ingress/src/surface.rs:1753-1760` (`served`, the reason "which no HTTP
  caller proves here") — cited
- **Files:** `crates/spindle-cli/src/serve.rs:358-360` (the loop printing `surface.report()`) — cited
- **Symbols:** `served` (surface.rs), `Surface::report`, `Unserved::Ingress`, `Route::not_served` — cited
- **Also likely:** a new accessor `Route::reads_caller` (or a field set in `served`) in
  `crates/spindle-ingress/src/surface.rs`, so `run` can name its flags without the ingress text
  naming CLI flags — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/caller.rs`, new
  `the_refusal_names_how_to_serve_a_caller_reading_command` (one `run` of the caller fixture
  without either setting; asserts the stderr line names `--fixed-actor`/`--fixed-caller` and
  `--trusted-caller-header`/`--trusted-caller-secret`; one process start, well under 60 s) — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/ingress.rs:457`
  (`a_non_loopback_address_without_the_trusted_proxy_is_refused_before_listening`) already holds
  `an_exposed_run_still_requires_the_proxy_secret`; keep it green or add the named test beside it.
  No new test binary (modules of `tests/main.rs`); test step stays under 120 s — cited
- **Tests kept green:** `crates/spindle-cli/tests/spindle-cli/caller.rs:110-146`
  (`a_route_whose_command_reads_the_caller_stays_unserved`) asserts the substring "this ingress
  does not serve the caller attributes it reads (`owner_id`)"; an appended clause keeps it — cited
- **Documents:** `CHANGELOG.md`; `docs/design/spindle.md` § 11 row only under decision C — inferred
- **Decision D1, default A:** A. the ingress reason stays free of CLI flags (it is shared with
  `crates/spindle-embed/src/lib.rs:174`, which has no CLI flags), and `run` prints one more stderr
  line after the report naming both settings; B. the ingress text at surface.rs:1755 names the CLI
  flags directly (one line; the embed host, the 501 body and `GET /api/served` then name flags that
  do not exist there); C. the first branch, a loopback run trusting the caller header without the
  secret, which amends the accepted `architecture-decision-record:http-caller-proof` and so waits
  for an answer (architecture) — cited
- **Confidence:** high — the story names surface.rs `served`; the flag gating is read in main.rs and serve.rs
- **Would collide with:** any unit touching `served` in `crates/spindle-ingress/src/surface.rs`
  (the query-string refusal sits at surface.rs:1772, 17 lines below), and any unit touching
  `serve::serve` in `crates/spindle-cli/src/serve.rs` — cited
- **Safety fact:** an exposed run is unchanged because `main.rs:502-511` exits 2 before listening
  for an address off loopback without the header and the secret, a check this story does not touch
  under A or B — step 2 (`crates/spindle-cli/src/main.rs:502`), unproven

## Decided when scoped

Taken 2026-11-26 on the scoper's recommended option, each reversible, unless noted.

- D1 A: the ingress text stays flag-free; `run` prints one stderr line naming
