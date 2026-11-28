---
format: aep.planning-md/3
id: review-result:wave34-adversary-U1-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 34 U1
relations:
- reviews: story:release-artifacts-signed
revision: 1
---
I added 5 red cases in 2 findings, both caused by this unit's changes. Neither is a state anyone has been shown to reach, so both are rated INFEASIBLE. The new signer rule itself held against every attack in the brief, and the indentation checks give the same results under mawk.

```
unit: story:release-artifacts-signed (wave 34 U1), findings cover 87f92cca (bf3d4916 + round-3 cases)
verdict: INFEASIBLE
cases: executed 367→450, red 5
origin: introduced 2 / pre-existing 0 / undecided 0
wrote-outside-worktree: 1 directory (scratch, listed in part 6)
needs-coordinator: none
```

**1. Diff.** The tree is clean after the commit. `git show --stat 87f92cca`: `scripts/release-linux-binary.adversary-test.sh | 144 +++`, 1 file. It is a test file only; no implementation file was touched.

**2. Cases added** (new section `adversary_w34_u1_r3`, at lines 1871 to 2012)
- `adversary_w34_u1_r3_signer_is_the_exact_uri` (**green**, 54 cases):
  - Two control runs pass: a tag push with one tag-ref attestation, and a backfill with its own attestation plus the tag run's.
  - 34 bad signers, each next to a passing attestation and each naming the tagged commit, are refused on a tag push. The 19 that look like `refs/heads/main` are also refused on a backfill.
  - The bad signers cover:
    - another repository or a near-identical repository name;
    - another workflow file (`ci.yml`, `release.yaml`, `nested/`, `xrelease.yml`);
    - extra text after the ref (`main-x`, `mainx`, `main/x`, `?ref=x`, a trailing space, `\n`, `0.26.0x`, `0.26.00`, `-rc.1`, `/x`) or before the address (a leading space, an `x`, `http://`);
    - a cut-off or longer tag (`0.26.`, `10.26.0`, and `0.26.0@refs/heads/main`);
    - the dots of the tag as regex wildcards (`0x26x0`);
    - 6 changes of upper and lower case;
    - signers that are not a string: `""`, `null`, an array, an object.
- `adversary_w34_u1_r3_one_json_document` (**red**, 3 rows). The output it was written for, from the first run (the script has no case filter, so the first run after writing was the whole script):
  ```
  FAIL: adversary_w34_u1_r3_one_json_document: a tag push passes when gh attestation verify prints two documents, the last one passing: [{"verificationResult":{"signature":{"certificate":{"buildSignerURI":"https://github.com/other/spindle/...
  FAIL: ... the last one passing: [] [{"verificationResult":...
  FAIL: ... the last one passing: null [{"verificationResult":...
  ```
- `adversary_w34_u1_r3_permission_at_any_depth` (**red**, 2 rows):
  ```
  FAIL: adversary_w34_u1_r3_permission_at_any_depth: a copy of release.yml with a job 'deep' holding "id\x2dtoken": write and no if passes adversary_id_token_only_on_signing_jobs
  FAIL: ... a job 'flow' holding "id\x2dtoken": write and no if passes ...
  ```
  `yq` confirms that both copies parse to `jobs.pr-signer.permissions: {"id-token": "write"}`, with no `if`.
- `adversary_w34_u1_r3_facts_equal_under_each_awk` (**green**, 24 cases here): every awk found on PATH (mawk, gawk, nawk, original-awk) must read the same permission facts from `release.yml` and from all 23 copies the check refuses. Only gawk is installed here and on the build node. The CI image (ubuntu-24.04) normally installs mawk as its awk, so the case would compare it there.

**3. Suite runs**
- `slot task check:the_release_binary_checks_refuse_what_they_must`, the first run after the cases existed: `release-linux-binary-adversary: 465 cases, 5 failed`, task exit 201.
- I then ran the backfill rows only for the signers that look like its own ref, because the rule reads the signer the same way on both events. Rerun with `slot sh -c '... sh scripts/release-linux-binary.adversary-test.sh'`: `450 cases, 5 failed`, `script 23426 ms exit 1`.
- Timing on the node: the script without the r3 section took 11.4 s for 367 cases; with it, 23 to 33 s depending on load. That is under the 60 s bound. No single case is over 5 s; the r3 section runs about 70 verification runs at roughly 0.2 s each.

**4. Findings**
- **The JSON rule passes on several documents.** At `.github/workflows/release.yml:1055`, `jq -e` exits on the last document it reads. If gh printed a refused list followed by a passing one, the run would pass.
  - What reaches it: nothing found. `gh attestation verify --format json` prints one array.
  - Verdict INFEASIBLE, origin introduced.
  - Fix (not applied): `jq -e -s 'length == 1 and (.[0] | <rule>)'`.
- **The permission check stops at fixed indentation.** At `scripts/release-linux-binary.adversary-test.sh:1288` and `:1302`, quoted keys are checked only up to 8 spaces, and spelled keys only on permission lines at 4 and 6 spaces. A job whose keys sit at 8 spaces, or a job written on one line as a flow mapping, can hold `"id\x2dtoken": write` and pass.
  - What reaches it: only a deliberately escaped key. This is the same class as round 2's escaped key, which was fixed.
  - Verdict INFEASIBLE, origin introduced.
  - Fix (not applied): run `spelled()` and the quoted-key check on every line under `jobs:` outside `run:` blocks, and refuse a job line under `jobs:` that is not `^  <name>:$`, or a job whose keys are not at 4 spaces.

**5. Attacked and not broken**
- Signer from another repository or workflow file, extra text before or after the ref, case changes, and regex characters in the tag: all refused. The comparison is exact, between strings passed in with `--arg`.
- jq on non-arrays, `[1]`, `[null]` and nested arrays: refused.
- An empty `COMMIT`: refused.
- mawk 1.3.4 20200120, run in a Debian bookworm container: identical permission facts for `release.yml`, all 19 mutant copies and both new copies, and identical mutant generation.
- One gap that holds only if a tag run's own attestation step were missing: on a tag push, an attestation at `refs/heads/main` naming the tagged commit satisfies "one names the expected commit". Not reachable, because publish needs the sign job to succeed.

**6. Paths written outside the worktree:** `<local path>`. It holds `mawk-probe.sh`, `permissions.awk`, `release.yml`, `deep.yml`, `flow.yml`, `facts.mawk`, `facts.gawk`, `work/` (its `copies/`, `gawk/`, `mutant.sh`, `mutant.yml`, `permissions.awk`), `run1.log`, `run2.log` and `commit-msg.txt`. The container ran with `--rm`. A temporary `target/r3-before.sh` was removed, both in the worktree and on the node.

**7.**
```findings
- file: .github/workflows/release.yml
  line: 1055
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: jq -e takes its exit status from the last of several JSON documents, so gh output of a refused list followed by a passing one passes the run (adversary_w34_u1_r3_one_json_document, 3 rows red)
- file: scripts/release-linux-binary.adversary-test.sh
  line: 1288
  category: mutant
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: a job whose keys sit at 8 spaces, or one written as a single flow mapping, holding "id\x2dtoken" write and no if passes adversary_id_token_only_on_signing_jobs, because quoted and spelled keys are checked only at the fixed indentation (adversary_w34_u1_r3_permission_at_any_depth, 2 rows red)
```
