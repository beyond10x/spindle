---
format: aep.planning-md/3
id: review-result:wave26-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 26 U3
relations:
- reviews: story:run-fails-on-rejected-line
- reviews: story:clock-and-mint-in-every-mode
revision: 1
---
needs-revision

```
unit: story:run-fails-on-rejected-line, story:clock-and-mint-in-every-mode, head 5e07fda7e
verdict: CONFIRMED
cases: executed 461→469, red 5
origin: introduced 5 / pre-existing 0 / undecided 0
```

Red on 5e07fda7e (`crates/spindle-cli/tests/spindle-cli/adversary_w26_u3.rs`):

- A fraction of a second in `--clock` or a stdin line's `at` was read as nanoseconds: `instant` took
  the fraction from the zero-trimmed `to_rfc3339()` spelling, so `.5` became 5 ns.
- A seeded mint did not avoid identities kept state holds: a second stdin run, or a served restart,
  with one `--mint-seed` on one `--state` failed every create that collided with the first run's.
- A served run on an environment binding `identity.mint = seeded` was refused even with
  `--mint-seed`, while the refusal told the user to give `--mint-seed`.

Notes: `json_object`'s doc line sat on top of `instant`; a rejected stdin line still moved the clock
to its `at`, though a rejected line did not run.

Fixed in b26c03b37 (correction round 1): the fraction padded to 9 digits; `Host::seeded` and the
stdin mint avoid the identities in the stores (`avoiding_identities`, as `reset_to` does); a served
run on the `seeded` binding listens when `--mint-seed` is given; the doc lines; a rejected line
leaves the clock where it was (`a_rejected_line_leaves_the_clock_where_it_was`, red first). All 5
cases green; spindle-cli 467 passed with the 3 ui/dist cases red as on the base.

```findings
- file: crates/spindle-cli/src/cli.rs
  category: boundary
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: instant() read the fraction from a zero-trimmed spelling, so .5 became 5 ns
- file: crates/spindle-host/src/host.rs
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a seeded mint did not avoid identities already in kept state
- file: crates/spindle-cli/src/main.rs
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a served run on identity.mint = seeded was refused even with --mint-seed
```
