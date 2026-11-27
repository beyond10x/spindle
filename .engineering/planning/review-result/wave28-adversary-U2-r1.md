---
format: aep.planning-md/3
id: review-result:wave28-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U2
relations:
- reviews: story:structured-json-logs
revision: 1
---
```
unit: story:structured-json-logs, head c7c826d9 (adversary commit fbc000e9 on unit/w28-json-logs)
verdict: CONFIRMED
cases: executed 547→552, red 1 (my cases; the suite also shows the 3 cases that already fail on the base)
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths (part 6)
needs-coordinator: none
```

I found one break. Before a served run starts listening, it prints a line for each operation it does not serve ("not served: …" and "unsupported outcome …"). In JSON mode these lines name a command in the message but carry no `command` field. The flag's own help says the fields appear "where the line names them". The 5 acceptance tests the story names all pass.

**1. Files changed** (`git --no-pager diff --stat c7c826d9 HEAD`). All three are test files; I touched no implementation code.
```
 .../fixtures/adversary-w28-u2-refused-http.yaml    |  11 +
 .../tests/spindle-cli/adversary_w28_u2.rs          | 370 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
```

**2. Cases** in `<local path>`. Each runs well under 5 s; the whole module took 0.21 s.
- `a_report_line_names_its_command_in_a_field`: **red.** On the `partial` fixture with `--log-format json`, every "not served:" or "unsupported outcome" line must carry a `command` field naming the operation. Output from running the module alone:
  ```
  panicked at crates/spindle-cli/tests/spindle-cli/adversary_w28_u2.rs:254:9:
  the line names an operation and carries no `command` field naming it: {"level": String("warn"), "message": String("not served: command `notes.pad.CopyNote` at POST /pad/commands/CopyNote: this ingress does not serve copied, which answers with the command's response"), "time": String("2026-11-27T07:52:07.946Z")}
  ```
- `time_is_the_instant_each_line_was_written`: green. Every `time` lies between the run's start and the moment the test read it. This catches a variant of the code that always writes the epoch fallback in `log::rfc3339`; the unit's `object()` check would let that through.
- `the_error_a_served_run_exits_with_is_one_json_line`: green. Exit 1 prints exactly one JSON object at level `error`, without the `b10x-spindle: ` prefix.
- `the_non_loopback_refusal_is_one_json_line`: green. Exit 2 prints one JSON object at level `error`.
- `a_refused_binding_keeps_its_text_and_is_one_json_object`: green. It uses the new fixture `adversary-w28-u2-refused-http.yaml`, a binding ESS refuses. In text mode the refusal has no prefix; this catches dropping `.bare()`, which no other test does. In JSON mode the multi-line refusal is one object.
  - My first version of this case used `reserved-prefix-http.yaml`. It went red because that is a serve-time refusal, not the binding refusal I meant to test. That was a mistake in my case, not a finding, so I rewrote it before committing.

**3. Suite**, run after the cases existed: `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast`
```
running 585 tests
test adversary_w28_u2::a_report_line_names_its_command_in_a_field ... FAILED
test deploy::the_ui_is_served_from_the_binary ... FAILED
test observe_panel::the_panel_is_one_self_contained_page ... FAILED
test observe_panel::the_panel_may_be_framed ... FAILED
test result: FAILED. 548 passed; 4 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.21s
exit=101 wall=43s
```
The last three failures also fail on the base, because the build node has no `ui/dist`.
- The 547 "before" figure comes from the filtered run of my module alone: 580 tests filtered out, minus the 33 ignored.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings`: exit 0.
- `rustfmt --check` on the module: exit 0, after formatting it with `slot --fetch`.

**4. The finding**
- **What was measured:** `adversary_w28_u2.rs:254`. The write at `crates/spindle-cli/src/serve.rs:382` (`log::warn(line).write()`) adds no field. The promise is in the help at `crates/spindle-cli/src/cli.rs:404`, repeated in `CHANGELOG.md` and `website/docs/reference/cli.md`.
- **What reaches it:** any served run of a spec that has an operation it cannot serve, which every such start prints by default. Origin is introduced, since JSON mode does not exist on the base.
- **Fix, which I did not apply:** either have the report give the operation as its own value, so the CLI can add `command`, or reword the help and CHANGELOG to say which lines carry fields.

**5. Attacked and not broken**
- Text-mode wording: I compared all 25 rewrapped strings with the base; every one matches byte for byte.
- JSON on a run that reads stdin exits 1 before reading stdin.
- No `eprint` is left on the served path in `serve.rs`, `seed.rs` or `main.rs`, and the other crates write nothing to stderr.
- The drop line carries `binding` only when the command came from a binding.
- `--log-format` conflicts with `--scenario`.
- The panic hook is not covered by any test, and I found no way to make the binary panic, so it stays unproven.

**6. Paths written outside the worktree** (all under `<local path>`):
- the directory itself
- `run1.log`
- `run2.log`
- `suite.log`
- `clippy.log`
- `commit-msg.txt`

```findings
- file: crates/spindle-cli/src/serve.rs
  line: 382
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: in JSON mode the "not served:" and "unsupported outcome" lines name an operation but carry no command field, though the help at cli.rs:404 promises the field wherever a line names one
```
