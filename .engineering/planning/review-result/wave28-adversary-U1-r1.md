---
format: aep.planning-md/3
id: review-result:wave28-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 28 U1
relations:
- reviews: story:console-routes-off
revision: 1
---
unit: story:console-routes-off. My cases are on top of head af672a90 (adversary commit 2cd59349, branch unit/w28-console-off)
verdict: NEEDS-CHANGE
cases: executed 547→550, red 2
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 3 paths (part 6)
needs-coordinator: none

**1. Diff stat (af672a90..2cd59349).** All three paths are test files.
```
 .../adversary-w28-u1-console-path-http.yaml        |  10 +
 .../tests/spindle-cli/adversary_w28_u1.rs          | 278 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
```

**2. Cases added** in `crates/spindle-cli/tests/spindle-cli/adversary_w28_u1.rs`. I ran them alone before running the suite. Each finishes in well under 1 s.

- `off_loopback_the_run_itself_serves_no_console_route_by_default`: **green**, kept as a passing probe.
  - It binds `::ffff:127.0.0.1`. `exposed` counts that address as off loopback, but only this host can connect to it.
  - With no flag and no setting, all four console routes answer 404 `no route serves <path>`.
  - With `--console on`, and with `SPINDLE_SETTING_CONSOLE=on`, they answer 200.
  - `--console off` with the setting at `on` gives 404, so the flag wins.
  - The unit's own case tested the default only through `console_served`. A `main.rs` that passed a fixed loopback address would have stayed green there; this case catches it.
- `with_the_console_off_a_console_path_answers_404_even_where_a_binding_route_carries_it`: **red**. It uses the fixture `reserved` with a binding whose route is `/{a}/{b}`.
  ```
  assertion `left == right` failed: --console off: GET /api/model answers as an unrouted path
    left: (200, Object {"rows": Array []})
   right: (404, Object {"refused": String("no route serves /api/model")})
  ```
- `the_setting_in_the_environment_does_not_refuse_a_scenario_run`: **red**. It runs `run --scenario` on the fixture `replay` with `SPINDLE_SETTING_CONSOLE=off` set.
  ```
  assertion `left == right` failed: SPINDLE_SETTING_CONSOLE=off on `run --scenario`: error: the argument '--scenario <SCENARIO>' cannot be used with '--console <SWITCH>'
    left: Some(2)
   right: Some(0)
  ```

**3. Suite run, after the cases existed.** `cargo test --locked -p spindle-cli --test spindle-cli --no-fail-fast` exited 101 after 46 s of wall time.
```
test result: FAILED. 545 passed; 5 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.20s
```
- The 5 failures are my 2 red cases and the 3 that the brief says are red on the base (`deploy::the_ui_is_served_from_the_binary`, `observe_panel::the_panel_is_one_self_contained_page`, `observe_panel::the_panel_may_be_framed`).
- "Before" (547) is the same run with my 3 cases taken out.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` exits 0, and `cargo fmt -p spindle-cli -- --check` exits 0.

**4. Findings** (tree at 2cd59349)

| file:line | verdict | origin | what was measured | what reaches it |
|---|---|---|---|---|
| `crates/spindle-cli/src/cli.rs:271` | NEEDS-CHANGE | introduced | `--console` has `env = "SPINDLE_SETTING_CONSOLE"` together with `conflicts_with = "scenario"`. clap counts a value from the environment as explicit, so `run --scenario` exits 2 whenever that variable is set (`adversary_w28_u1.rs:266`). | The flag's own text tells operators to set it. The process launcher exports one `SPINDLE_SETTING_<NAME>` per setting (`spindle-target/src/process.rs:25`). So a shell or image that sets it and then runs `run --scenario` fails. Fix: drop `conflicts_with` for this flag (it is already "only where run serves HTTP"), or reject only when the value comes from the command line. |
| `crates/spindle-cli/src/cli.rs:262` | NEEDS-CHANGE | introduced | The flag's text, CHANGELOG and README say that with the console off each console path "answers `404` as an unrouted path does". A binding route that matches a console path (`/{a}/{b}`) instead answers `GET /api/model` with 200 `{"rows": []}` (`adversary_w28_u1.rs:237`). `cli.rs:163-164` still says such a route "exits 1 before listening", which is now true only with the console on. | Any `--http` binding with a route template under `/api/`. It is reachable by default off loopback, because there the console is now off. The same binding exits 1 on loopback and starts off loopback. Fix: either keep the shadowing refusal no matter the switch, or correct the flag text, `cli.rs:163`, CHANGELOG and README. The `surface.rs` doc the unit edited already describes the actual behaviour. |

**5. Attacked and not broken**
- Every way to resolve a console route goes through `Surface::resolve`, which checks `self.console`. Nothing in the router, the listener or the reset path in `serve.rs:505/519` reaches the console without it.
- There is one production `with_console` call. `component.rs`'s ingress never adds the console.
- Defaults: an unspecified address (`0.0.0.0`, `::`) or `::ffff:127.0.0.1` means off; `127.x` and `::1` mean on.
- With the console off, a row written before the refused reset is kept, and the published routes are still served.

**6. Paths written outside the worktree**
- `<local path>`
- `<local path>`
- `<local path>`

```findings
- file: crates/spindle-cli/src/cli.rs
  line: 271
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: SPINDLE_SETTING_CONSOLE in the environment counts as an explicit --console for clap's conflicts_with, so run --scenario exits 2 wherever the setting is exported
- file: crates/spindle-cli/src/cli.rs
  line: 262
  category: contract-drift
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: with --console off a binding route carrying a console path answers it (GET /api/model 200) where the flag text, CHANGELOG and README promise 404, and cli.rs:163 still says such a binding exits 1
```
