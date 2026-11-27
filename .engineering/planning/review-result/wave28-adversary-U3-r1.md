---
format: aep.planning-md/3
id: review-result:wave28-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U3
relations:
- reviews: story:metrics-series-bounded
revision: 1
---
unit: story:metrics-series-bounded, head e52846b0 (implementation ce3e2c6a plus my test commit)
verdict: CONFIRMED
cases: executed 172→174, red 1
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 path
needs-coordinator: is the list of seven labels in D1 complete? The `refusal` label holds a refusing outcome the specification declares, and the implementor treated the list as complete.

**1. Diff stat** (`git --no-pager diff --stat ce3e2c6a..HEAD`). Both paths are test files:
```
 .../tests/spindle-ingress/adversary_w28_u3.rs      | 241 +++++++++++++++++++++
 .../spindle-ingress/tests/spindle-ingress/main.rs  |   1 +
```

**2. Cases added** in `crates/spindle-ingress/tests/spindle-ingress/adversary_w28_u3.rs`. Both run on the existing fixture `tests/fixtures/adversary-w27-u7` and post 10, 0, 500 and 5000.

- `an_aggregated_scrape_writes_no_refusing_outcome_the_specification_declares` is **red**. It first checks that the per-command scrape counts refusals as `empty`, `unchanged`, `flagged` and `large`. It then asserts that the aggregated scrape writes none of those outcome names as a label value. Output when run alone, before the suite:
  ```
  the aggregated scrape writes the declared outcomes ["empty", "unchanged", "flagged", "large"] as label values:
  [
      "spindle_bound_refusals_total{binding=\"\",command=\"\",refusal=\"broken_invariant\"} 1",
      "spindle_bound_refusals_total{binding=\"\",command=\"\",refusal=\"unchanged\"} 1",
      "spindle_bound_refusals_total{binding=\"\",command=\"\",refusal=\"empty\"} 1",
      "spindle_bound_refusals_total{binding=\"\",command=\"\",refusal=\"large\"} 1",
      "spindle_bound_refusals_total{binding=\"\",command=\"\",refusal=\"flagged\"} 1",
  ]
  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 151 filtered out; finished in 0.01s
  ```
- `aggregating_keeps_every_family_total` is **green and kept**. For the same posts, every family has the same total in both modes, and the aggregated scrape writes fewer series. It would catch a change that overwrites series instead of summing them.
- Each case takes 0.01 s.

**3. Suite runs, after the cases existed:**
- `cargo test --locked -p spindle-ingress --no-fail-fast`, exit 101:
  - lib: `ok. 22 passed`
  - `tests/spindle-ingress/main.rs`: `FAILED. 151 passed; 1 failed ... finished in 11.22s`
- Same command with `-- --skip adversary_w28_u3`, exit 0: lib `22 passed`, integration `150 passed; 0 failed; ... 2 filtered out; finished in 12.69s`. That gives the 172 "before" count.
- `cargo fmt -p spindle-ingress -- --check`: exit 0.
- `cargo clippy --locked -p spindle-ingress --all-targets -- -D warnings`: Finished, no warnings.

**4. Findings** (they cover e52846b0)
- **F1** (acceptance, warning, CONFIRMED, introduced). `crates/spindle-ingress/src/observe/metrics.rs:224`: `SPECIFIED_LABELS` leaves out `refusal`.
  - **Effect:** Acceptance 1 says every label whose value comes from the specification is written empty. In aggregated mode, `spindle_bound_refusals_total` still writes one series for each refusing outcome the specification declares, so it grows with the specification.
  - **What reaches it:** any `run --observe --observe-series aggregated` on a specification whose event bindings call commands that refuse.
  - **Fix, not applied:** write `refusal` empty when it holds a declared outcome and keep the kernel's error kinds, or add `refusal` to `SPECIFIED_LABELS`.
  - `website/docs/concepts/observe.md` admits the label is kept, so how severe this is depends on how the coordinator reads D1.
- **F2** (contract-drift, note, CONFIRMED, introduced). `crates/spindle-cli/src/cli.rs:345` (and so `website/docs/reference/cli.md`), `README.md:116` and `CHANGELOG.md:14`.
  - **Claim:** these say that in aggregated mode the number of series "does not depend on the specification's size", or that a specification of 300 commands writes as many series as one of 30.
  - **Measured:** that is false for `refusal` (F1 measures it) and for `component`, whose values are the component names the specification declares. The concepts page states both limits; these three texts do not.
  - **What reaches it:** an operator reading `--help` or the README.

**5. Attacked, not broken:**
- `--observe-series` without `--observe` exits 2.
- A bad value exits 2.
- A conflict with `--scenario` is refused.
- The default is `per-command`.
- `serve.rs` passes the mode through, so a test that ignored it would fail.
- `observing(capacity)` stays per-command, and `metrics_read` pins the mode line.
- Histogram counts and sums add up correctly.
- Route edges in the topology still count in aggregated mode (the unit's CLI test covers this).
- `exposition` has one caller.
- The unit test covers every family.
- No ESS parity applies.

**6. Paths written outside the worktree:**
- `<local path>` (the commit message)

**7.**
```findings
- file: crates/spindle-ingress/src/observe/metrics.rs
  line: 224
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: aggregated mode keeps spindle_bound_refusals_total's refusal label, which holds an outcome the specification declares, so the scrape writes one series per declared refusing outcome (adversary_w28_u3::an_aggregated_scrape_writes_no_refusing_outcome_the_specification_declares is red)
- file: crates/spindle-cli/src/cli.rs
  line: 345
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the --observe-series help, README.md:116 and CHANGELOG.md:14 say aggregated series do not depend on the specification's size, while component names and refusing outcomes from the specification still add series
```
