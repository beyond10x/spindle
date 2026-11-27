---
format: aep.planning-md/3
id: review-result:wave30-adversary-U9-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U9
relations:
- reviews: story:rally-example-reads-stdin
revision: 1
---
```
unit: story:rally-example-reads-stdin (wave 30 U9); findings cover head b4f05bc3, adversary commit 9ba76dcb on unit/w30-rally
verdict: CONFIRMED
cases: executed 620→628, red 3 (plus the 3 known ui/dist cases)
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 4 paths (part 6)
needs-coordinator: none
```

**The unit's own claim holds, but `--stdin` misses some served-only flags.** The rally page's output block matches a real run byte for byte. However, `--observe-requests` and `--observe-series` beside `--stdin` are accepted and ignored (exit 0). `--stdin --log-format json` exits 1 or 3 instead of the 2 that `run --help` promises.

## 1. Diff stat (b4f05bc3..9ba76dcb)

```
 .../tests/spindle-cli/adversary_w30_u9.rs          | 272 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
 2 files changed, 273 insertions(+)
```

Only test files changed.

## 2. Cases added

All in `<local path>`. The `mod` line is in `main.rs` in sorted order.

| case | asserts | now |
|---|---|---|
| `the_page_output_block_is_byte_equal_to_the_run` | stdout equals the page's console block byte for byte, exit 1 | green |
| `stdin_with_log_format_text_given_reads_the_lines` | an explicit `--log-format text` is accepted, same stdout, line 3 named | green |
| `stdin_ignores_the_console_flag_and_its_setting` | `--console on`, and `SPINDLE_SETTING_CONSOLE=on`, do not stop the run | green |
| `stdin_with_scenario_exits_2` | clap conflict, nothing on stdout | green |
| `stdin_on_a_spec_without_a_networked_component_reads_the_lines` | the stdin-lines fixture: `AddBook` answers `added`, exit 0 | green |
| `stdin_refuses_the_observe_flags_that_require_observe` | `--observe-requests 5` and `--observe-series aggregated`, beside `--stdin`, exit 2 | **red** |
| `stdin_with_log_format_json_exits_2_where_no_component_is_networked` | stdin-lines fixture, `--stdin --log-format json` exits 2 (as `--port 0` does) | **red** |
| `stdin_with_log_format_json_exits_2_before_admission` | rally on `browser.toml`, `--stdin --log-format json` exits 2 with nothing on stdout | **red** |

Red output from running the module alone (`cargo test --locked -p spindle-cli --test spindle-cli -- adversary_w30_u9`), before the suite ran:

```
---- adversary_w30_u9::stdin_with_log_format_json_exits_2_where_no_component_is_networked stdout ----
assertion `left == right` failed: Output { status: ExitStatus(unix_wait_status(256)), stdout: "", stderr: "{\"level\":\"error\",\"message\":\"nothing in crates/spindle-cli/tests/fixtures/stdin-lines requires ingress.network, so --log-format json would not be kept: only a run serving HTTP writes its log as JSON lines\",\"time\":\"2026-11-27T11:32:04.833Z\"}\n" }
  left: Some(1)
 right: Some(2)
---- adversary_w30_u9::stdin_with_log_format_json_exits_2_before_admission stdout ----
assertion `left == right` failed: Output { status: ExitStatus(unix_wait_status(768)), stdout: "{\"missing\":[\"timer\"],\"outcome\":\"refused\"}\n", stderr: "" }
  left: Some(3)
 right: Some(2)
---- adversary_w30_u9::stdin_refuses_the_observe_flags_that_require_observe stdout ----
assertion `left == right` failed: examples/rally/ess ["--observe-requests", "5"] is a flag only a served run reads: Output { status: ExitStatus(unix_wait_status(0)), stdout: "", stderr: "b10x-spindle: state is kept in /srv/slot/tmp/.tmpFIsWkf, removed when the run ends\n" }
  left: Some(0)
 right: Some(2)
test result: FAILED. 5 passed; 3 failed; 0 ignored; 0 measured; 653 filtered out; finished in 0.19s
```

Every case ran in under 1 s.

## 3. Suite run (after the cases existed)

- `SLOT_TIMEOUT=10m cargo test --locked -p spindle-cli --no-fail-fast`:
  - unit tests: `test result: ok. 5 passed`
  - integration binary: `test result: FAILED. 622 passed; 6 failed; 33 ignored; ... finished in 33.16s`
  - The 6 red are my 3 plus the 3 ui/dist cases that are red on the base.
  - The "before" count of 620 is 628 minus my 8 cases, all of which were selected. I did not do a separate run without them.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: finished, no warnings.
- `cargo fmt -p spindle-cli --check`: exit 0, after formatting my file with `slot --fetch … rustfmt`.

## 4. Findings

**F1: `--observe-requests` and `--observe-series` slip past `--stdin`.**
- Measured: `adversary_w30_u9.rs:184`, exit 0 where 2 is expected, on rally and on the stdin-lines fixture.
- Without `--stdin`, either flag exits 2 because it needs `--observe`. With `--stdin`, clap excuses the missing `--observe` because `--stdin` conflicts with it (`clap_builder` `is_missing_required_ok`). The run then reads stdin and ignores the flag.
- The same mechanism lets `--stdin --fixed-caller '{}'` reach `serve::callers`, which exits 1 instead of 2.
- What reaches it: a user who adds `--stdin` to a served command line that has these flags.
- Cause: `crates/spindle-cli/src/cli.rs:279`.
- Fix to name: add `observe_requests`, `observe_series` and `fixed_caller` to `conflicts_with_all`. Each has a default value, so a conflict fires only when the flag is given explicitly.

**F2: `--stdin --log-format json` exits 1 where no component is networked.**
- Measured: `adversary_w30_u9.rs:207`.
- The new `--stdin` help says this combination exits 2. On a spec with no networked component it reaches the old message and exits 1, while `--stdin --port 0` on the same spec exits 2.
- Cause: `crates/spindle-cli/src/main.rs:810`. The exit-2 branch is gated on `networked.is_some()`.

**F3: `--stdin --log-format json` is refused only after admission.**
- Measured: `adversary_w30_u9.rs:242`. On `browser.toml` it prints the admission and exits 3. The check sits after spec load, admission, decisions and the seed check (`main.rs:691` → `:810`).
- This contradicts "exits 2 before running anything".
- An environment `[seed]` file also exits 1 before this check is reached.
- Fix to name: refuse `stdin && log_format == Json` unconditionally at the top of `execute`, or in `main` before dispatch. JSON on stdin is refused on every spec anyway.

**Judgement, no case written:**
- Origin of F1 to F3 is `introduced`: on the base no `--stdin` existed, and without `--stdin` each of these refuses with exit 2.
- Note: the exit-2 refusal of `--log-format json` (`main.rs:813`) is itself written as a JSON log line, because `main` calls `log::set(Json)` first. The message says JSON "would not be kept" while being written as JSON, and it differs from clap's plain-text usage errors.

## 5. Attacked and could not break

- The page's output block against a real run: byte-equal, exit 1, stderr names line 3 only.
- The 9 flags clap conflicts with: each exits 2. An explicit `--drain 20`, which equals the default, also exits 2.
- `--stdin --stdin`: exit 2.
- `--log-format text` given explicitly: accepted.
- `--console on` and `SPINDLE_SETTING_CONSOLE=on` are ignored, as `--console`'s help documents. A bad value exits 2 from clap.
- `--scenario` together with `--stdin`: exit 2.
- A spec with no networked component under `--stdin`: reads its lines.
- `laptop-processes.toml` (channel tcp) under `--stdin`: same output as the page.
- `postgres.toml`: exit 1 with a clear message.
- `node.toml`: admission refuses, exit 3.
- A `--state` directory: exit 1, "Is a directory".
- `--mint-seed` with empty stdin: exit 0.
- Mutants:
  - Dropping `.filter(|_| !stdin)` makes the page run serve: the unit's page test catches it.
  - Dropping the networked log-json branch: the unit's served-only-flags test catches it.

## 6. Paths written outside the worktree

- `<local path>`: `probe.sh`, `probe2.sh`, `commit-msg.txt`, plus the probe outputs written there (`*.out`, `*.err`, `requests.jsonl`, `page-out.txt`, `rally.sqlite`, `seeded.toml`, `seed.json`, `statedir/`).
- Temporary state directories the binary made under `<local path>`. The binary removes them when each run ends.

## 7. Findings block

```findings
- file: crates/spindle-cli/src/cli.rs
  line: 279
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: --stdin excuses the --observe that --observe-requests and --observe-series require, so beside --stdin both are silently ignored with exit 0 where without --stdin they exit 2 (test adversary_w30_u9.rs:184)
- file: crates/spindle-cli/src/main.rs
  line: 810
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: --stdin --log-format json exits 1 on a spec with no networked component, against the exit 2 run --help promises and --stdin --port gives on the same spec (test adversary_w30_u9.rs:207)
- file: crates/spindle-cli/src/main.rs
  line: 691
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: --stdin --log-format json is refused only after spec load and admission, so an environment that refuses the spec prints the admission and exits 3 instead of 2 before running anything (test adversary_w30_u9.rs:242)
- file: crates/spindle-cli/src/main.rs
  line: 813
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the exit-2 refusal of --log-format json beside --stdin is itself written as a JSON log line, unlike clap's plain-text usage errors for the other nine flags
```
