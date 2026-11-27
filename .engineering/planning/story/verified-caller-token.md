---
format: aep.planning-md/3
id: story:verified-caller-token
kind: story
status: proposed
title: A served run verifies a signed caller token itself
summary: Each request's OIDC/JWT token is verified against the issuer's JWKS and its claims mapped to the actor and caller attributes.
tags:
- needs-owner-acceptance
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: architecture-decision-record:http-caller-proof
- informed_by: story:http-caller-proves-actor
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.lock
- confidence: inferred
  path: crates/spindle-cli/Cargo.toml
- confidence: inferred
  path: crates/spindle-cli/src/calls.rs
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/src/token.rs
- confidence: inferred
  path: crates/spindle-cli/tests/fixtures/trusted-caller
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/caller_token.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: inferred
  path: crates/spindle-ingress/src/answer.rs
- confidence: inferred
  path: crates/spindle-ingress/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: cited
  path: crates/spindle-ingress/src/surface.rs
- confidence: cited
  path: docs/design/spindle.md
- confidence: inferred
  path: ess/components.yaml
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/capabilities.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 9
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
---
# A served run verifies a signed caller token itself

## Outcome

A served `run` verifies a signed token on each request (OIDC/JWT, signature checked against the
issuer's JWKS, issuer, audience and expiry checked) and maps its claims to the actor and the
caller attributes of the model, so no request proves a caller by a header any proxy could copy.

## Why

Today a caller is proven only by `--trusted-caller-header`: a JSON value a proxy sets, trusted
because the proxy also sends a shared secret (`crates/spindle-cli/src/cli.rs:244-262`;
`Caller::proven`, `crates/spindle-ingress/src/surface.rs:142-158`, parses the value and verifies
nothing). A consumer's proxy copies a value the browser chose into that header, so any user acts
as any actor. `architecture-decision-record:http-caller-proof` chose the proxy header and kept
"OIDC tokens checked in Spindle" (option B) open as another source of the same caller.

## Acceptance

- `a_request_with_a_valid_token_runs_as_its_mapped_caller`: a token signed by a key of the
  configured JWKS, with the configured issuer and audience, runs a command that reads the caller
  as the actor and attributes its claims map to.
- `a_bad_token_is_refused_and_writes_nothing`: an expired token, a wrong issuer or audience, an
  unknown key id, a signature that does not verify, `alg: none`, and no token each answer `401`
  and move no row, event or counter.
- `a_claim_that_maps_to_no_caller_of_the_model_is_refused`: a token whose claims name an
  undeclared actor or leave out a required attribute answers `403` "not granted".
- `keys_rotate_without_a_restart`: a key added to the JWKS after start verifies a token after the
  next refresh; a removed key no longer does.
- `a_token_run_refuses_the_trusted_caller_header_beside_it`: the token flag given with
  `--trusted-caller-header` exits 1 before the listening line (D2 A).
- `a_token_run_off_loopback_exits_2`: a token run whose `--address` is off loopback exits 2 before
  the listening line (D3 A).
- `a_jwks_unreachable_at_start_exits_1`: a JWKS that cannot be fetched at start exits 1 before the
  listening line (D5 A).
- `the_console_routes_are_off_on_a_token_run`: on a token run `--console on` exits 2, and under
  `/__spindle/` only `live`, `ready` and `seed` answer without a token (D4 B).
- `a_mapping_naming_a_symmetric_algorithm_is_refused`,
  `a_jwks_url_neither_https_nor_loopback_http_is_refused` and
  `a_jwk_of_another_family_than_the_token_alg_does_not_verify`: unit tests in `token.rs`.

The process tests are in `crates/spindle-cli/tests/spindle-cli/caller_token.rs`, on the fixture
`trusted-caller`, with the JWKS served on loopback inside the test.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: `jsonwebtoken` 9.3.1 with `default-features = false`, on the `ring` 0.17 and `base64` 0.22
  already in the lock, so `jsonwebtoken` is the only new crate; answered A by the host operator on 2026-11-27, and this
  default stands. Not taken: 10.4.0 on `rust_crypto` (`rsa` 0.9 carries
  RUSTSEC-2023-0071), on `aws_lc_rs` (a C build on all five release targets), or on a ring provider
  written in Spindle.
- D2 A: one caller source per run: the token flag given with `--trusted-caller-header` exits 1; the
  header flag keeps working on its own.
- D3 A: a token run off loopback exits 2, as a run without the proxy flags does today; no
  `depends_on story:listener-tls`, whose TLS a later story may let lift this.
- D4 B: the scoper's recommended option here is B: the console is off and `--console on` exits 2;
  under `/__spindle/*`, `live`, `ready` and `seed` answer without a token and every other path needs
  a verified token.
- D5 A: `refresh_seconds` in the mapping file, 300 by default, 1 to 86400; a failed first fetch
  exits 1 before listening; a later failed refresh keeps the last good key set and writes one
  warning line.
- D6 A: a TOML mapping file with `issuer`, `audience`, `jwks` (https, or http to loopback only, as
  `calls.rs` allows), optional `ca_file`, `algorithms` (asymmetric only), `leeway_seconds`,
  `refresh_seconds`; `[actor] claim` with `[actor.values]` mapping a claim value to a qualified
  actor; `[attributes."<actor>"]` mapping each attribute to a top-level claim.
- D7 A: a `401` is neither recorded nor counted under `--observe`, as the proxy-secret refusal is
  not (`listener.rs:597-603`); `spindle-requests/1` and the metrics families are unchanged.

## Needs the owner's acceptance

The JWKS fetch is an outbound connection a run makes on its own.

## Architecture decision

Option A, picked by the operator on 2026-11-27: a JWT checked against a JWKS beside
`--trusted-caller-header`, which keeps working. New dependency `jsonwebtoken`, a claim-mapping
file, and a periodic JWKS fetch. The pick accepts the JWKS fetch as an outbound connection a run
makes on its own.

Not taken: B (the token replaces the header flag), C (no change; the docs tell the proxy what to
strip), D (defer).

## Scope

Derived 2026-11-27 by `story-scoper` on `plan/wave-31` at 8eaea47a. Every line is **cited** (read from the story or the tree) or **inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` listener proof path, plus a new token verifier and JWKS refresher in `crates/spindle-cli` — cited (story; `listener.rs:693-702` is the one place a request's headers become a caller)
- **Files:** `crates/spindle-ingress/src/listener.rs:581-705` (`answer`: the `proven` match at :693-702, the unrecorded secret refusal at :597-606 as the model for an unrecorded 401), `:749-767` (`proven`, `presents`, `once`) — cited
- **Files:** `crates/spindle-ingress/src/surface.rs:1127-1145` (`Surface` fields beside `caller_header`/`caller_secret`), `:1320-1372` (`proving_callers`, `proving`, `caller_secret`, `caller_header`), `:1189-1203` (`Reaching::proving_callers`), `:182-214` (`Caller`, `Caller::proven`) — cited
- **Files:** `crates/spindle-cli/src/cli.rs:342-368` (the new flag beside `--trusted-caller-header`), `:284-293` (the `--address` rule), `:766-771` (`console_served`, D4) — cited
- **Files:** `crates/spindle-cli/src/main.rs:215-252` (`Run` destructure into `Execute`), `:595-676` (`Execute` fields and the flag checks), `:817-823` (the "nothing requires ingress.network" refusals) — cited
- **Files:** `crates/spindle-cli/src/serve.rs:128-133` (`Serving` fields), `:372-389` (surface construction), `:394-405` and `:436-445` (startup lines) — cited
- **Symbols:** `Caller::proven`, `Surface::proving_callers`, `Reaching::proving_callers`, `Surface::caller_secret`, `Surface::secret_required`, `Answer::not_proven`, `answered_as`, `console_served` — cited
- **Also likely:** new `crates/spindle-cli/src/token.rs` (mapping file, `jsonwebtoken` verification, JWKS refresh thread on its own current-thread tokio runtime; `mod token;` in `main.rs:3-17`) — inferred
- **Also likely:** `crates/spindle-cli/src/calls.rs:386-419` (`client`, `file_roots`, `platform_roots` made `pub(crate)` for the JWKS fetch over the same ring TLS client; `file_roots`' error names `[http.<name>]` and needs a caller-supplied label) — inferred
- **Also likely:** `crates/spindle-ingress/src/answer.rs:60-68` (a new `Answer::unauthenticated`, `401` with `WWW-Authenticate: Bearer`); `answered_as` `:287-340` unchanged (an unmapped token reaches it as `proven: None`) — inferred
- **Also likely:** `crates/spindle-ingress/src/lib.rs:94-96` (re-export of the verifier trait or type) — inferred
- **Also likely:** `crates/spindle-cli/Cargo.toml` and `Cargo.lock` (`jsonwebtoken`, D1; also as a dev-dependency to sign test tokens) — inferred
- **Tests:** new `crates/spindle-cli/tests/spindle-cli/caller_token.rs` plus its line in `crates/spindle-cli/tests/spindle-cli/main.rs`, on the existing fixture `crates/spindle-cli/tests/fixtures/trusted-caller` (Member with `person_id`, Auditor); JWKS served over loopback `http://` by a thread in the test; keys generated per test with `rcgen` (already a dev-dependency, ring), no private key committed — inferred
- **Documents:** `docs/design/spindle.md` § 11, a new row after the caller-header row at `:427` — cited (`architecture-decision-record:http-caller-proof` writes caller-proof decisions there)
- **Documents:** `website/docs/reference/cli.md` (generated from clap by `crates/spindle-docs/src/cli.rs`), `crates/spindle-docs/src/status.rs:450-456` with the generated `website/data/status.json` and `website/docs/status.md`, `website/docs/concepts/capabilities.md:115-125`, `ess/components.yaml:37-43` (comment), `CHANGELOG.md` — inferred
- **Confidence:** medium — the proof site, the flag site and the 403 path are read from the tree; the split between ingress and cli (verifier trait in ingress, `jsonwebtoken` in cli) and D1–D7 are open
- **Would collide with:** units touching `spindle-ingress` `listener.rs` request handling or `surface.rs` constructors and fields; `spindle-cli` `run` flags (`cli.rs`, `main.rs` `execute`), `serve.rs` surface construction and startup lines; `calls.rs` TLS helpers; design § 11; the generated CLI and status pages
- **Safety fact:** the browser and wasm builds stay unaffected, because `spindle-ingress` is built without the `listener` feature there (`crates/spindle-wasm/Cargo.toml:22`, `crates/spindle-embed/Cargo.toml:18`, `crates/spindle-node/Cargo.toml:22`) and `jsonwebtoken` lives only in `spindle-cli`; the field holding the verifier on `Surface` must be a dependency-free trait object behind a wrapper with hand-written `Debug`/`PartialEq`/`Eq`, as `Surface` derives them (`surface.rs:1126`) — 2, unproven
