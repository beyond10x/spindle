---
format: aep.planning-md/3
id: story:listener-tls
kind: story
status: proposed
title: The ingress listener serves TLS, and may require a client certificate
summary: A served run serves HTTPS with a given certificate and, with a CA file, requires a client certificate (mTLS).
relations:
- serves: vision:spindle
- decomposes: epic:production-service
- informed_by: story:decision-calls-tls
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 3}
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
  exit 2 before the listening line.

## Decided when scoped

- Reload of a renewed certificate without a restart, and the minimum TLS version.

## Architecture decision

Option A, picked by the operator on 2026-11-27: server TLS with optional client certificates, on
`rustls` and `tokio-rustls`, which the workspace already holds, and three new flags.

Not taken: B (server TLS only), C (no change, TLS in a sidecar), D (defer).
