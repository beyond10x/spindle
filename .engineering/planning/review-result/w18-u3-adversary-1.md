---
format: aep.planning-md/3
id: review-result:w18-u3-adversary-1
kind: review-result
status: active
title: Wave 18 U3 adversary pass 1
relations:
- reviews: story:external-cause-bound-to-http-call
revision: 1
---
needs-revision

```
unit: wave 18 U3 story:external-cause-bound-to-http-call, commit f2446927 on f9c99a67, worktree spindle-w18-http-cause plus two uncommitted test-file changes
verdict: NEEDS-CHANGE
cases: executed 303→307, red 3
origin: introduced 3 / pre-existing 0 / undecided 0
```

Cases added in `crates/spindle-cli/tests/spindle-cli/adversary_http_cause.rs`:

- `a_url_carrying_userinfo_never_puts_the_password_on_the_wire_as_host`: red. `the URL's password is sent as the host header: host: desk-user:desk-password@127.0.0.1:43213`
- `a_huge_reply_is_not_held_in_memory_whole`: red. `a 268435456-byte reply raised the run's peak resident size to 290 MiB`
- `a_cause_answered_by_its_http_call_is_listed_in_the_observed_request`: red. `"externals":[]`, `left: 0 right: 1`
- `a_served_call_that_fails_answers_501_and_writes_nothing`: green.

Suite: `cargo test -p spindle-cli --test spindle-cli`: `test result: FAILED. 301 passed; 6 failed; 4 ignored` (3 of the 6 are the known missing ui/dist cases), exit 101.

Findings:

- `crates/spindle-cli/src/calls.rs:242`, `:186`: a URL's `user:password@` goes out as the `Host` header and a bind error prints the URL with `{url:?}`, against the story's "No secret is written in … an environment document". Fix: refuse userinfo, set `Host` from `host[:port]`. NEEDS-CHANGE, introduced.
- `crates/spindle-cli/src/calls.rs:257`: `collect()` buffers the whole reply with no limit, while ingress bounds its bodies (`Limited`, `BODY_LIMIT`). Fix: `http_body_util::Limited`. NEEDS-CHANGE, introduced.
- `ess/domains/observe.yaml:402`: the contract lists every external cause asked, a 501 as `unanswered`; `calls.over` wraps the observed decider from outside (`crates/spindle-cli/src/serve.rs:309`, `:334`), so a call's answer or failure is never recorded or counted in `spindle_external_causes_total`. CONFIRMED, introduced.
- `crates/spindle-cli/src/calls.rs:213`: each call blocks the single served host thread, including the SIGTERM poll (`serve.rs:355`), for up to `timeout_ms`, which has no upper bound. CONFIRMED by reading, note, introduced.

Attacked and not broken: a failed served call answers 501 and writes nothing; the token is absent from the 501 body and every error message, the header value is sensitive and `Call` has no `Debug`; the timeout covers connect to the last body byte; `holds` as a string or null is refused; redirects are not followed; `https://` is refused at bind; a cause is asked once per run (`execute.rs:1201`); no nested `block_on`; a document without `[http]` serialises without the key.

```findings
- file: crates/spindle-cli/src/calls.rs
  line: 242
  category: boundary
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a url with userinfo is accepted and its user:password is sent as the Host header, putting a document-held credential on the wire where proxies and access logs record it
- file: crates/spindle-cli/src/calls.rs
  line: 257
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the reply body is collected with no size limit, so a 256 MiB reply raised the run's peak resident size to 290 MiB within the timeout
- file: ess/domains/observe.yaml
  line: 402
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a cause answered or failed by its HTTP call is never listed in the observed request's externals or counted in spindle_external_causes_total, against the contract's every-cause-asked wording
- file: crates/spindle-cli/src/calls.rs
  line: 213
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: each call blocks the single served host thread, including the signal poll, for up to timeout_ms, which has no upper bound
```
