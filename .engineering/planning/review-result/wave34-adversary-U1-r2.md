---
format: aep.planning-md/3
id: review-result:wave34-adversary-U1-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 34 U1
relations:
- reviews: story:release-artifacts-signed
revision: 1
---
```
unit: story:release-artifacts-signed (wave 34 U1), findings cover ef5bcc54 (256384e2 plus my test commit)
verdict: CONFIRMED
cases: executed 336→353, red 4
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 12 paths, listed in part 6
needs-coordinator: under decision D-U1-r1, should a second backfill fail on the first backfill's attestation? (judgement finding 3)
```

Round 2 broke the fixed guard parser in two ways, so the release-check task is red. One of the two is a mistake someone could make by accident. The ref step and the commit rule held. I wrote kept cases for both, and they pass.

**1. Diff stat** (`git --no-pager diff --stat 256384e2..HEAD`)
```
 scripts/release-linux-binary.adversary-test.sh | 118 +++++++++++++++++++++++++
 1 file changed, 118 insertions(+)
```
Only the test file changed. I committed it as ef5bcc54 on `unit/w34-release-signed`.

**2. Cases added** in the new section `adversary_w34_u1_r2` of `<local path>`, starting at line 1695.

- **`adversary_w34_u1_r2_guard_continued_line`** (line 1701, 2 cases, **red**)
  - The copy puts the line `      && needs.provenance.result == 'success'` under the `if:` of `sign`, and under the `if:` of `image-manifest`. YAML reads this as one value with text after the `}}`.
  - With text outside `${{ }}`, the runner turns the `if:` into a `format()` string. That string is never empty, so the job also runs on a pull request.
  - I checked the folding with yq and Ruby's YAML parser: both read `${{ ... }} && needs...` as one value.
  - The unit's check only reads the `if:` line itself (`:1277`, the `" }}"` test at `:1248`), so both copies pass.
- **`adversary_w34_u1_r2_escaped_permission_key`** (line 1724, 2 cases, **red**)
  - The copy gives the `ui` job, which runs on pull requests, either `"id\x2dtoken": write` or `permissions: "write\x2dall"`.
  - The patterns `/id-token/` and `/write-all/` (`:1274-1275`) look for the literal text, so the escaped spellings pass.
- **`adversary_w34_u1_r2_ref_step_always_runs`** (line 1742, 4 cases, green)
  - It asserts that provenance's first step is the ref step, and that neither that step nor the `provenance` job has an `if:` or `continue-on-error:`.
  - It refuses three copies: a step-level `if: ${{ github.event_name != 'workflow_dispatch' }}`, a step-level `continue-on-error: true`, and a job-level `continue-on-error: true`.
  - A probe showed the unit's own check passes all three copies: the first-step test at `:1574` passes, and the run block it extracts is unchanged.
- **`adversary_w34_u1_r2_commit_rule_fails_closed`** (line 1774, 9 cases, green)
  - Baseline: a tag push with one attestation of the tagged commit passes.
  - The step then fails, as it should, when gh prints any of: `[]`, `null`, `{}`, `[{}]`, `not json`, nothing, a null digest, or one object instead of a list.

First execution, with the red output verbatim. The script runs as one unit, so "the case alone" means this whole run. In it, two red messages named the wrong job because my loop variable was overwritten by `id_token_breach`; I fixed the message text only.
```
FAIL: adversary_w34_u1_r2_guard_continued_line: a copy of release.yml whose image-manifest if goes on over a second line after its }} (always true, on a pull request too) passes adversary_id_token_only_on_signing_jobs
FAIL: adversary_w34_u1_r2_guard_continued_line: (same, second copy)
FAIL: adversary_w34_u1_r2_escaped_permission_key: a copy of release.yml whose ui job holds  passes adversary_id_token_only_on_signing_jobs
FAIL: adversary_w34_u1_r2_escaped_permission_key: a copy of release.yml whose ui job holds  permissions: "write\x2dall" passes adversary_id_token_only_on_signing_jobs
release-linux-binary-adversary: 353 cases, 4 failed
```

**3. Suite run** after my cases existed: `slot task check:the_release_binary_checks_refuse_what_they_must`, exit 201, 12.8 s total.
```
FAIL: adversary_w34_u1_r2_guard_continued_line: a copy of release.yml whose sign if goes on over a second line after its }} (always true, on a pull request too) passes adversary_id_token_only_on_signing_jobs
FAIL: adversary_w34_u1_r2_guard_continued_line: a copy of release.yml whose image-manifest if goes on over a second line after its }} (always true, on a pull request too) passes adversary_id_token_only_on_signing_jobs
FAIL: adversary_w34_u1_r2_escaped_permission_key: a copy of release.yml whose ui job holds  "id\x2dtoken": write passes adversary_id_token_only_on_signing_jobs
FAIL: adversary_w34_u1_r2_escaped_permission_key: a copy of release.yml whose ui job holds  permissions: "write\x2dall" passes adversary_id_token_only_on_signing_jobs
release-linux-binary-adversary: 353 cases, 4 failed
```
The count went from 336 (from the implementor's commit 256384e2) to 353. My 17 cases ran.

**4. Findings**

1. **Guard on a continued line** — `scripts/release-linux-binary.adversary-test.sh:1248` and `:1277`. Category mutant, warning, CONFIRMED, introduced.
   - **What reaches it:** wrapping a long `if:` onto a second line after its `}}`. That is a plausible edit, and the result is always true.
   - **Fix:** fail when the line after a job's `if:` is deeper indented and is not a new key.
2. **Escaped permission spellings** — `scripts/release-linux-binary.adversary-test.sh:1274-1275`. Category mutant, note, INFEASIBLE, introduced.
   - **What reaches it:** only a deliberate obfuscation. A pull-request author who writes it also controls this check, so nobody reaches it by accident.
   - **Fix:** refuse any `\` escape or quoted key in a `permissions:` block or value.
3. **A second backfill fails** — `.github/workflows/release.yml:1056` (the `all(...)` part of D-U1-r1). Category judgement, warning, CONFIRMED, introduced.
   - Backfill 1 runs from main at A, attests in `sign`, then fails before the upload. Someone dispatches backfill 2 from main at B. The archives are rebuilt byte-identical, so gh returns attestations naming A and B, and A is neither the tagged commit nor B.
   - The jq probe returns `false`, exit 1. That run then fails after its upload, and a later run cannot replace the assets.
   - "Re-run failed jobs" keeps commit A and passes; a fresh dispatch does not.
   - I wrote no red case because this follows from the decided rule. It needs your decision.

**5. Attacked, not broken**
- **Quoted literals:** the `"` handling cannot hide a `||`, because GitHub expressions have no double-quoted strings, and `''` is handled correctly.
- **Other ways to hide `||`:** `format()`, `contains()` or `fromJSON()` inside the `&&` chain cannot make the expression true on a pull request. A comment after `}}`, a CRLF ending, a duplicate `if:` or a block scalar (`>-`) are all refused.
- **YAML anchors:** an `&anchor` on a `permissions:` line marks the block as "other", which is refused.
- **Skipping `provenance` through `needs:`:** `sign` and `image-manifest` only run after `provenance` succeeds, through `gate`, `build` and `image-push`, which have no status function in their `if:`.
- **The ref step itself:** an empty `GITHUB_EVENT_NAME` fails under `set -u`.
- **Commit rule:** with `COMMIT` empty it fails closed. Two JSON documents in a row pass jq's `-e` (it judges the last one), but gh prints one document, so this is not reachable.

**6. Paths written outside the worktree**, all under `<local path>`:
- `run1.log`, `run2.log`, `commit-msg.txt`
- `cont.yml`, `esc.yml`
- `ref-step-if.yml`, `ref-step-if.sh`, `ref-step-continue.yml`, `ref-step-continue.sh`, `ref-job-continue.yml`, `ref-job-continue.sh`
- `sum`

**7. Findings block**
```findings
- file: scripts/release-linux-binary.adversary-test.sh
  line: 1248
  category: mutant
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the guard check reads only the if line, so an if continued on a deeper indented line after its closing }} (a format() string, always true, on a pull request too) passes for sign and image-manifest
- file: scripts/release-linux-binary.adversary-test.sh
  line: 1275
  category: mutant
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: 'an escaped id-token key or write-all value on a pull-request job passes the check because it matches the literal text only; nobody reaches this except by deliberate obfuscation'
- file: .github/workflows/release.yml
  line: 1056
  category: judgement
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: under D-U1-r1 a backfill dispatched again after an earlier backfill attested and failed before upload finds that run's attestation (a third commit) and fails verification after its own upload
```
