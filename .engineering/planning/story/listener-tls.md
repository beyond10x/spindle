---
format: aep.planning-md/3
id: story:listener-tls
kind: story
status: active
title: The ingress listener serves TLS, and may require a client certificate
summary: A served run serves HTTPS with a given certificate and, with a CA file, requires a client certificate (mTLS).
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:decision-calls-tls
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.lock
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-cli/src/listener_tls.rs
- confidence: cited
  path: crates/spindle-cli/src/main.rs
- confidence: cited
  path: crates/spindle-cli/src/serve.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/listener_tls.rs
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-ingress/Cargo.toml
- confidence: cited
  path: crates/spindle-ingress/src/lib.rs
- confidence: cited
  path: crates/spindle-ingress/src/listener.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/main.rs
- confidence: inferred
  path: crates/spindle-ingress/tests/spindle-ingress/tls.rs
- confidence: inferred
  path: website/data/status.json
- confidence: inferred
  path: website/docs/concepts/capabilities.md
- confidence: inferred
  path: website/docs/reference/cli.md
- confidence: inferred
  path: website/docs/status.md
revision: 12
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-27T13:07:33Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":4}}}
---
# The ingress listener serves TLS, and may require a client certificate

## Outcome

A served `run` given a certificate and key serves HTTPS on its listener, and given a CA file
requires each client (the proxy in front) to present a certificate that CA signed (mTLS).

## Why

The listener binds a plain TCP socket (`crates/spindle-ingress/src/listener.rs:313`) and serves
HTTP/1.1 over it as it is (`connection`, `listener.rs:377-385`). Between a consumer's proxy and
Spindle every caller value and the proxy secret cross the network in clear text. Outbound calls
already use rustls on `ring` (`story:decision-calls-tls`); the listener does not.

## Acceptance

- `a_tls_listener_answers_https_and_refuses_plain_http`: with a certificate and key, a request over
  TLS verified against the issuing CA is answered; a plain HTTP request gets no HTTP answer.
- `a_client_certificate_is_required_when_a_ca_is_given`: with a client CA, a client without a
  certificate, or with one another CA signed, fails the handshake; one the CA signed is answered.
- `a_bad_certificate_file_exits_before_listening`: an unreadable or mismatched certificate and key
  exit 2 (`USAGE`, D3 A) before the listening line. An unreadable file writes an error line
  `--tls-cert <path>: ...` (or `--tls-key <path>`, `--tls-client-ca <path>`) naming the flag and
  the file; a mismatched pair writes one naming both files and saying the key does not match the
  certificate. The test reads that line, so clap's own exit 2 for an unknown flag does not pass it.
- `a_served_run_given_a_certificate_answers_https`: `run` given `--tls-cert` and `--tls-key`
  answers HTTPS end to end (D6 A).

The first two are in-process tests in `crates/spindle-ingress/tests/spindle-ingress/tls.rs`, the
last two process tests in `crates/spindle-cli/tests/spindle-cli/listener_tls.rs`; certificates are
generated at run time with `rcgen`, on loopback only.

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: a renewed certificate is picked up by a restart; no reload.
- D2 A: TLS 1.2 and 1.3 (rustls `with_safe_default_protocol_versions`), the set the outbound client
  offers.
- D3 A: a bad certificate, key or CA file exits 2 through an explicit `USAGE` return in `execute`,
  as the acceptance says; today `main()` turns every other error into exit 1.
- D4 A: with a client CA every connection presents a certificate, `/__spindle/live` and
  `/__spindle/ready` included; probes use `exec` or `tcpSocket`, and the docs say so.
- D5 A: off loopback `--trusted-caller-header` and `--trusted-caller-secret` stay required; mTLS
  adds to them.
- D6 A: `--tls-cert <FILE>`, `--tls-key <FILE>`, `--tls-client-ca <FILE>`; certificate and key
  require each other and the CA requires both (clap `requires`, exit 2); no environment variable.
- D7 A: the listening line is unchanged, `{"listening", "components"}`.

## Architecture decision

Option A, picked by the operator on 2026-11-27: server TLS with optional client certificates, on
`rustls` and `tokio-rustls`, which the workspace already holds, and three new flags.

Not taken: B (server TLS only), C (no change, TLS in a sidecar), D (defer).

## Scope

Derived 2026-11-27 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-ingress` listener (`src/listener.rs`) — cited
- **Files:** `crates/spindle-ingress/src/listener.rs`: `Listener::serve` `:380-460` (plain bind `:395`, accept loop `:429-436`), `connection` `:530-550` (`serve_connection(TokioIo::new(stream))` `:539`); a new constructor beside `bind_reading_on` `:366-378`, taking an `Arc<rustls::ServerConfig>`, so the 13 existing callers of `bind`/`bind_reading`/`bind_reading_on` stay unchanged — cited (story; stale lines corrected)
- **Files:** `crates/spindle-ingress/Cargo.toml`: `rustls` and `tokio-rustls` as optional deps under the `listener` feature (`:11-13`), `rcgen` as a dev-dep — cited
- **Files:** `crates/spindle-ingress/src/lib.rs`: the `listener` re-export `:93` and the module doc `:31-41` — cited
- **Files:** `crates/spindle-cli/src/serve.rs`: a `Serving` field beside `proxy_secret` (`:128-133`); `Listener::bind_reading_on` at `:498` becomes the TLS constructor when the field is set — cited
- **Files:** `crates/spindle-cli/src/cli.rs`: three flags in `Command::Run` beside `--address` (`:280-295`) and `--trusted-caller-secret` (`:368`) — cited
- **Files:** `crates/spindle-cli/src/main.rs`: the `Command::Run` destructure (`:213-258`), `Execute` (`:588-622`), the early refusals in `execute` (`:647-677`, where an exit-2 refusal goes), and the `serve::Serving` literal (`:759-781`) — cited
- **Also likely:** `crates/spindle-cli/src/listener_tls.rs` (new): the PEM certificate, key and client CA read with `rustls::pki_types::pem::PemObject` as `calls.rs:398-411` reads a CA, and the `ServerConfig` built on `ring` with `WebPkiClientVerifier` — inferred
- **Also likely:** `Cargo.lock`, only `spindle-ingress`'s dependency list; no new package — inferred
- **Symbols:** `Listener::serve`, `connection`, `Listener::bind_reading_on`, `serve::Serving`, `Execute`, `USAGE` — cited
- **Tests:** `crates/spindle-ingress/tests/spindle-ingress/tls.rs` (new) with `a_tls_listener_answers_https_and_refuses_plain_http` and `a_client_certificate_is_required_when_a_ca_is_given`: in-process, CA and certificates generated at run time with `rcgen` as `spindle-cli/tests/spindle-cli/tls_cause.rs:35-60` does, a blocking rustls client on loopback; no PostgreSQL, container or outside network; default test step; each expected well under 60 s — inferred
- **Tests:** `crates/spindle-cli/tests/spindle-cli/listener_tls.rs` (new) with `a_bad_certificate_file_exits_before_listening` (an unreadable file and a mismatched certificate and key, each exiting before the listening line) and `a_served_run_given_a_certificate_answers_https` (the flags wired end to end): process tests like `tls_cause.rs`, loopback only, default test step, each expected well under 60 s — inferred
- **Tests:** one `mod` line each in `crates/spindle-ingress/tests/spindle-ingress/main.rs` and `crates/spindle-cli/tests/spindle-cli/main.rs` — inferred
- **Documents:** `website/docs/concepts/capabilities.md` `:115-125` (the `--address` and proxy paragraph), `website/docs/reference/cli.md` (generated from `cli.rs`), `CHANGELOG.md` `[Unreleased]` — inferred
- **Documents:** `crates/spindle-docs/src/status.rs` (a row for the new capability; `AGENTS.md:149` asks a capability change to change the site) with the generated `website/data/status.json` and `website/docs/status.md` — inferred (added 2026-11-27 on the plan critics' finding)
- **Confidence:** high for the listener and the `run` flag path (the story names the listener, and `serve.rs:498` is the only call site `run` uses); medium for the new files' names and the split between `spindle-cli` and `spindle-ingress`
- **Would collide with:** `crates/spindle-cli/src/main.rs` around `execute`'s ingress branch (wave 30 U9 `:724-842` contains the `Serving` literal `:759-781`); `crates/spindle-cli/src/cli.rs` `Command::Run` flags and the generated `website/docs/reference/cli.md` (wave 30 U9, U12); `crates/spindle-cli/src/serve.rs` `Serving` (wave 30 U12 `:139-141`, `:457-465`); `website/docs/concepts/capabilities.md` `:115-125` (wave 30 U12 `:123`); `crates/spindle-ingress/src/lib.rs` module doc (wave 30 U12 `:43-50`); the test `main.rs` lists of `spindle-ingress` (U3) and `spindle-cli` (U9); `CHANGELOG.md`. Among wave-31 candidates: `story:verified-caller-token` (the same flags, `Execute`, `Serving`, likely `listener.rs` and `spindle-ingress/Cargo.toml` plus `Cargo.lock` for `jsonwebtoken`, `capabilities.md`); `story:event-egress-and-dead-letters` and `story:several-writers-one-postgres` (`serve.rs`, `cli.rs`, `main.rs`; the latter maybe the listener's readiness)
- **Safety fact:** a listener given no TLS config serves exactly as today, because the TLS path is a separate arm of `Listener::serve` (`listener.rs:380`) reached only from a new constructor; and the TLS crates are optional under `listener` (`spindle-ingress/Cargo.toml:11-13`), so the wasm builds that take `spindle-ingress` with `default-features = false` (`spindle-wasm/Cargo.toml:22`, `spindle-node/Cargo.toml:22`, `spindle-embed/Cargo.toml:18`) do not pull in `ring` — step 2, unproven
- **Safety fact:** the handshake must run inside the per-connection task spawned at `listener.rs:431`, bounded by `HEADER_READ` (`:37`) and watching `closing`, so a client that stalls its handshake neither blocks the accept loop nor holds the drain past its grace — step 2, unproven
- **Safety fact:** a certificate and key that do not match are refused by rustls 0.23.46 itself: `with_single_cert` calls `CertifiedKey::from_der`, which checks `keys_match()` (`rustls-0.23.46/src/crypto/signer.rs:159-172`) — step 2, unproven
