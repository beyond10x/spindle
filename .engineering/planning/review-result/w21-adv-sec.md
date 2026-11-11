---
format: aep.planning-md/3
id: review-result:w21-adv-sec
kind: review-result
status: active
title: Wave 21 security, adversary pass 1
relations:
- reviews: story:command-grant-before-request
- reviews: story:unauthenticated-grant-refusals
- reviews: story:refusal-reason-bounded
- reviews: story:state-directory-lock
- reviews: story:release-dependency-list
revision: 1
---
needs-revision

Adversary pass on wave 21's security merges: grants (dc99a1bf), bounded reasons (2dd8ba88), the
state lock (431a5a36) and the auditable release (9904b8f4); cases on review/w21-adv-sec, commit
1c6777be. All nine findings fixed in 58167b40, c6cd6bf2 and b6017e16.

```findings
- file: crates/spindle-ingress/src/answer.rs
  line: 262
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a granted command that reads its caller, served without a fixed caller, answers 501 naming its caller attributes before the grant check instead of the 403 not-granted refusal the CHANGELOG states
- file: crates/spindle-ingress/src/observe.rs
  line: 800
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a map key the caller sent survives in the request-log reason as a backticked plain name, against "a value the caller sent is never recorded"
- file: crates/spindle-ingress/src/observe.rs
  line: 794
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a digit-only map key survives in the reason because a bracketed run of digits is kept as a list index
- file: crates/spindle-ingress/src/observe.rs
  line: 860
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a backtick inside a form or query value closes the elided span early and the rest of the value is copied into the reason verbatim
- file: crates/spindle-cli/src/state_lock.rs
  line: 29
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the stdin run's lock file is named after the uncanonicalized path, so a symlink to a held state file takes its own lock and opens the same database
- file: crates/spindle-ingress/src/surface.rs
  line: 1199
  category: judgement
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the Service Worker and component ingress build Surface::new with no way to fix or prove a caller, so every granted command and read-granted view there now answers 403, beyond the "run serving HTTP" scope the CHANGELOG names
- file: ess/domains/observe.yaml
  line: 392
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: a request-log record's path has no stated bound, so an unrouted request with a path near the URI limit adds about 64 KiB per record
- file: crates/spindle-ingress/src/answer.rs
  line: 353
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: unknown JSON, form and query keys are kept verbatim in the reason up to 512 bytes
- file: .github/workflows/release.yml
  line: 395
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the checksum check verifies a manifest computed one line earlier from the same files and cannot fail; hash in the build job and verify in publish
```
