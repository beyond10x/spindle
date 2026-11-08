---
format: aep.planning-md/3
id: review-result:w15-u1-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on ingress-prometheus-metrics (wave 15)
relations:
- reviews: story:ingress-prometheus-metrics
revision: 1
---
needs-change

Adversary pass 1 on story:ingress-prometheus-metrics (wave 15 U1), attacking a7de57b4. Three cases
in `crates/spindle-cli/tests/spindle-cli/adversary_prometheus.rs` (commit 402a23c6); the filtered
run went from 22 to 25 cases, 1 red.

- F1, in scope, fixed in the unit (e09e4668: a real `_sum` and `_count`; the telemetry histograms keep a sum, `spindle.observe.Histogram` gains `sum_us`, which widened the unit to `crates/spindle-telemetry`): the command-duration histogram wrote `_bucket` lines without
  `_count`; the 0.0.4 text format requires `x_count` equal to the `+Inf` bucket, and a parser that
  reads the count sees 0.
- F2, note: one store that cannot be scanned turns the whole scrape into a 500
  (`crates/spindle-ingress/src/observe/metrics.rs:315`); no fixture reaches it.
- F3, note: HEAD on `/__spindle/metrics` answers 405, as `/__spindle/requests` already does.
- F4, note: a reset that empties the stores and then fails loading its world is not counted in
  `spindle_resets_total` (`crates/spindle-ingress/src/router.rs:260`); no fixture reaches it.

Could not break: one console series for reset, counter monotonicity across a reset, the scrape not
counting itself, HELP and TYPE once per family, label escaping, cumulative buckets, a component
with no traffic, unknown methods as `other`, unmatched paths sharing one series, concurrent
requests counted once, every `Response` construction site.

```findings
- file: crates/spindle-ingress/src/observe/metrics.rs
  line: 341
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the command-duration histogram has no _count, against the 0.0.4 rule that the +Inf bucket equals x_count
- file: crates/spindle-ingress/src/observe/metrics.rs
  line: 315
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: one store that cannot be scanned turns the whole scrape into a 500
- file: crates/spindle-ingress/src/observe.rs
  line: 551
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: HEAD on /__spindle/metrics answers 405
- file: crates/spindle-ingress/src/router.rs
  line: 260
  category: boundary
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a reset that empties the stores and then fails loading its world is not counted in spindle_resets_total
```
