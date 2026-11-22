---
format: aep.planning-md/3
id: story:decision-calls-tls
kind: story
status: implemented
title: A decision call leaves the host only over verified TLS
relations:
- serves: vision:spindle
revision: 5
transitions:
- {from: "draft", to: "proposed", at: "2026-11-22T11:25:50Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-22T11:25:52Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-22T11:26:05Z", actor: "human:timo", revision: 5, decided_on: {"recorded":{"test_result":1}}}
---
# A decision call leaves the host only over verified TLS

## Outcome

An `[http.<name>]` call of an environment (the decider for `external:` causes) sends the
command's whole input and, when `secret_env` names one, a bearer token. In 0.17.0
`crates/spindle-cli/src/calls.rs:169` accepts only `http://` URLs and `send` (`calls.rs:263`)
writes the request on a plain TCP stream to any host. After this story:

- `https://` URLs are called over TLS with the server certificate verified against the platform's
  trust roots, or against the PEM file an optional `ca_file` key of the call names.
- `http://` is accepted only when the host is loopback (`localhost`, `127.0.0.0/8`, `::1`).
  Any other `http://` URL is refused when the environment is loaded, naming the call.
- A certificate that does not verify is a failed call, never an answer.

## Acceptance

- `an_off_host_http_call_is_refused`: `[http.x] url = "http://example.invalid/"` is refused at
  load, naming `http.x`, and no connection is opened.
- `a_loopback_http_call_is_still_made`: the existing loopback tests keep passing unchanged.
- `an_https_call_verifies_the_certificate`: a local TLS server with a test CA named by `ca_file`
  answers `{"holds": true}` and the cause holds; the bearer header arrives.
- `an_untrusted_certificate_fails_the_call`: the same server without `ca_file` fails the call and
  the reason names the certificate; the bearer token was never written to the stream.

## Origin

Spindle 0.17.0 review, 2026-11-22: "Remote decision providers".
