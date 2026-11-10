---
format: aep.planning-md/3
id: review-result:w18-u3-adversary-2
kind: review-result
status: active
title: Wave 18 U3 adversary pass 2
relations:
- reviews: story:external-cause-bound-to-http-call
revision: 1
---
needs-revision

```
unit: wave 18 U3 story:external-cause-bound-to-http-call, commits f2446927 + 594016f7 on branch unit/w18-http-cause (uncommitted test files only)
verdict: NEEDS-CHANGE
cases: executed spindle-cli 308→315, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary2_http_cause.rs`:

- `a_mistake_beside_a_credential_url_never_prints_the_credential`: red. An inline `[http]` table with `user:password@` in the URL and one TOML mistake on the line (negative timeout, missing `timeout_ms`, misspelt key): the refusal prints toml's source snippet, password included.
- `the_timeout_bound_holds_at_each_edge`: green (1 and 30000 accepted; 0, 30001, i64::MAX refused as `HttpTimeout`; -1, 1.5, `"100"`, 1e3, absent refused as `Parse`).
- `every_userinfo_form_is_refused_and_an_at_sign_elsewhere_is_not`: green.
- `an_ipv6_desk_named_with_an_uppercase_scheme_is_sent_host_and_port_only`: green.
- `a_reply_at_the_limit_answers_and_every_longer_or_malformed_reply_fails`: green (65536 answers; 65537, chunked over the limit, short Content-Length, cross-host 302, non-JSON 200 fail as `undecided`).
- `a_reply_dripped_a_byte_at_a_time_fails_at_the_timeout`: green.
- `every_call_is_listed_and_counted_once_across_failures_repeats_and_a_reset`: green.

Suite: `cargo test -p spindle-cli --test spindle-cli`: `FAILED. 311 passed; 4 failed; 4 ignored` (3 need a built ui/dist; the fourth is the red case).

Finding: `crates/spindle-capability/src/environment.rs:83-84,147`. `EnvironmentError::Parse` displays `toml::de::Error` whole, which quotes the source line. A TOML mistake on the line holding a URL (an inline table, a bad escape inside the URL string) prints the URL's credential to stderr before the `UrlUserinfo` refusal runs. Reached by every subcommand that reads the environment document. NEEDS-CHANGE, introduced, warning.

Attacked and not broken: the userinfo checks at document read and in `Calls::bind` agree; `Host` from host and port for IPv6 and explicit ports; `Calls` Debug, bind and call errors and metric labels carry no URL; CRLF in a method or secret is refused; `Limited` cuts one byte past the limit, chunked included; the timeout covers connect through the last body byte; calls are recorded once across failures, repeats and resets.

```findings
- file: crates/spindle-capability/src/environment.rs
  line: 84
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a TOML mistake on the line holding an [http] url (inline table, bad escape in the string) is refused with toml's source snippet, printing the url's user:password to stderr before the UrlUserinfo check runs
```
