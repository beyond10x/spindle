---
format: aep.planning-md/3
id: review-result:wave30-adversary-U5-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U5
relations:
- reviews: story:create-on-held-identity-unavailable
revision: 1
---
```
unit: story:create-on-held-identity-unavailable, findings cover head 11cd8104 (unit head 040a8b23 plus my test-only commit)
verdict: CONFIRMED
cases: executed 725→735, red 2
origin: introduced 0 / pre-existing 1 / undecided 0
wrote-outside-worktree: 1 directory (scratch logs) plus one worktree I made and then removed
needs-coordinator: the pre-existing finding needs an owner (U6's `Host::establish` hunk or a new story)
```

The unit holds everywhere I attacked the change itself. The only red cases come from an older defect: an `Integer` identity set up by a scenario's establish step is stored under a different key than the same integer sent by a command. So a create on that identity makes a second row on the host, where ESS's interpreter answers unavailable. The story's Outcome names this case ("held by an establish step"), so it is not met for `Integer` identities. The unit's diff does not cause it.

**1. Diff stat (`git diff --stat 040a8b23..HEAD`).** Every path is a test file or a test fixture, so no implementation file was touched.
```
crates/spindle-cli/tests/fixtures/adversary-w30-u5/{system.yaml,domains/box.yaml,domains/load.yaml}
crates/spindle-cli/tests/spindle-cli/adversary_w30_u5.rs        | 210
crates/spindle-cli/tests/spindle-cli/main.rs                    |   1
crates/spindle-conformance/tests/fixtures/adversary-w30-u5/{system.yaml,domains/box.yaml,domains/load.yaml}
crates/spindle-conformance/tests/spindle-conformance/adversary_w30_u5.rs | 322
crates/spindle-conformance/tests/spindle-conformance/main.rs    |   1
10 files changed, 952 insertions(+)
```
Commit 11cd8104, made with `b10x-gates bot -- commit -F`. The public-repo grep printed nothing.

**2. Cases added.** Each one runs on the host and on ESS's interpreter 0.99.2 and compares the full answer text and the view rows.

`crates/spindle-conformance/tests/spindle-conformance/adversary_w30_u5.rs`:

| Case (line) | Now |
|---|---|
| `:159` a command with two creating outcomes: the refusal names the guarded one, `PackBox/rushed` | green |
| `:183` an identity sent in the input, with a minted fallback | green |
| `:207` an identity the model writes as a literal | green |
| `:220` a second command on the same entity is named in the refusal | green |
| `:271` the same identity in another component's entity is free, and is held after it is created | green |
| `:303` the create after a refusal is unaffected | green |
| `:244` `Integer` identity established as `7`, then created as `7` and as `7.0` | **red** |

`crates/spindle-cli/tests/spindle-cli/adversary_w30_u5.rs`:

| Case (line) | Now |
|---|---|
| `:114` replay of a guarded held create prints `diverges, unavailable` and ESS's words; ESS's runner status is `Error` | green |
| `:179` stdin `run`: the second create keeps kind `undeclared` and the kernel's message (the story says stdin does not change) | green |
| `:147` replay of a create on an established `Integer` identity | **red** |

Red output of `:244`, run alone (`cargo test -p spindle-conformance --test spindle-conformance -- adversary_w30_u5::w30_u5_an_integer`):
```
assertion `left == right` failed: host left, interpreter right
  left: ["established", "opened ", "error: unavailable invoking `shelf.box.OpenTally`: … already held by `shelf.box.Tally`, and creation never replaces it", "shelf.box.AllTallies: [{\"label\":\"a\",\"tally_id\":7.0},{\"label\":\"established\",\"tally_id\":7.0}]"]
 right: ["established", "error: unavailable invoking `shelf.box.OpenTally`: … already held by `shelf.box.Tally`, and creation never replaces it", "error: unavailable …", "shelf.box.AllTallies: [{\"label\":\"established\",\"tally_id\":7.0}]"]
```
Red output of `:147`, run alone:
```
assertion `left == right` failed: Output { status: ExitStatus(unix_wait_status(0)), stdout: "arrange: holds\ntimeline[0] shelf.box.OpenTally: holds, outcome opened\n", stderr: "" }
  left: Some(0)
 right: Some(1)
```
A consumer reaches this with an authored `ess-scenario/2` that establishes an `Integer`-identity row and then creates the same id. ESS's runner fails that scenario; Spindle passes it and exits 0.

**3. Suite runs, after the cases existed.**
- `cargo test --locked -p spindle-conformance --no-fail-fast` exited 101 in 17 s: `test result: FAILED. 108 passed; 1 failed; 6 ignored` (the 1 is `:244`).
- `cargo test --locked -p spindle-cli --no-fail-fast` exited 101 in 35 s: `test result: FAILED. 620 passed; 1 failed; 33 ignored` (the 1 is `:147`), plus `5 passed` in the lib tests. The three `ui/dist` cases were green on this node.
- `cargo clippy --locked -p spindle-conformance -p spindle-cli --all-targets -- -D warnings` exited 0. `rustfmt --check` is clean on both new files.
- The "before" count of 725 is these same runs minus my 10 cases: 102 conformance plus 623 CLI.
- Every new case runs in under 1 s.

**Base check.** I made a managed worktree at 8eaea47a (`adv-w30-u5-base`), copied the cases in and ran them:
- Conformance `:244` is red there too: step 1 answers `opened ` and the view holds two rows with `tally_id` 7. The other six conformance cases are red on the base only because the base still answers `<undeclared>`, which is the fix this unit makes.
- CLI `:147` is red there too: exit 0, `holds, outcome opened`.

Then I removed my copies, ran `worktree finish --discard-cache` and `worktree gc --id adv-w30-u5-base --apply`. The tree is gone.

**4. Findings that are not failing cases** (both at 11cd8104):
- `CHANGELOG.md:62-71`, contract drift, origin introduced. In `conform` and `run --scenario`, a scenario with a held create now ends `error`, not `failed`: ESS's runner sends any unavailable answer to `target_failure` (ess-conformance `runner.rs:4378`). `Run::passed_with` (`crates/spindle-conformance/src/lib.rs:406-413`) requires that no scenario ends `error`. So a consumer whose known-failures file names such a scenario will see the run stop passing. This matches ESS's interpreter, but the CHANGELOG entry does not say it. Reached by any consumer that runs `conform` with a known-failures file. Found by reading the code, not by a case.
- `crates/spindle-kernel/src/lib.rs:234`, judgement, origin introduced. A new public field on a variant that is not `#[non_exhaustive]` breaks any crate that builds or fully destructures `ExecuteError::Undeclared`. The CHANGELOG lists it under Fixed. Reached by downstream crates only; I found none in this repository besides the ones the unit updated.

Pre-existing defect: `crates/spindle-conformance/src/establish.rs:40` passes the identity through unchanged to `Host::establish` (`crates/spindle-host/src/host.rs:1304`), which stores it under the spelling it came in with (it shows as `7.0` in the views). Commands rewrite `Integer` input to the integer (`execute.rs`, in `reaching`). The likely fix is to apply the kernel's integer spelling to the identity in `Host::establish`. U6 changes that function for `Decimal`, so it could cover `Integer` too.

**5. Attacked and could not break:**
- The detail text against ESS 0.99.2 `select`, `create` and `branch()`, for the guarded outcome, another command and its outcome name.
- Supplied identities through a minted fallback and through a literal.
- An identity used by another component's entity.
- No leftover state after a refusal: the next create, the rows and the answer are unchanged.
- Stdin: kind and message unchanged.
- The replay line and the check printed under it.
- Several outcomes selected at once happens in ESS only for open external branches or several default branches; I built no such case.
- A create with `affects:`: the kernel's create path computes no affected rows (`execute.rs` `take`), so I left it.
- Identities minted from a seed or random cannot be compared with ESS's counter, so I left them.

**6. Paths written outside the worktree:** `<local path>`, which holds `conf1.log`, `conf2.log`, `conf3.log`, `cli1.log`, `suite-conf.log`, `suite-cli.log`, `clippy.log`, `base-conf.log`, `base-cli.log` and `commit-msg.txt`. The worktree `<local path>` was created and is now removed.

**7.**
```findings
- file: crates/spindle-conformance/src/establish.rs
  line: 40
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: pre-existing
  message: an Integer identity established by a scenario is stored under a different key than the same integer sent by a command, so a create on it makes a second row and the run passes where ESS's interpreter answers unavailable (red in adversary_w30_u5 :244 in spindle-conformance and :147 in spindle-cli)
- file: CHANGELOG.md
  line: 62
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a scenario with a held create now ends error instead of failed in conform and run --scenario, which stops passed_with from passing for a known-failures file that names it, and the entry does not say so
- file: crates/spindle-kernel/src/lib.rs
  line: 234
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a new public field on the variant, which is not non_exhaustive, breaks downstream code that builds or fully destructures ExecuteError::Undeclared, yet the change is listed under Fixed
```
