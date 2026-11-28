---
format: aep.planning-md/3
id: review-result:wave34-adversary-U1-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 34 U1
relations:
- reviews: story:release-artifacts-signed
revision: 1
---
unit: story:release-artifacts-signed (U1). Findings cover the unit head 3c851646; my tests are commit aa5cd6bb on `unit/w34-release-signed`
verdict: CONFIRMED
cases: executed 312→317, red 3
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths (part 6)
needs-coordinator: D4 says `publish` gets no new permission, but `publish` runs `gh attestation verify` without `attestations: read`. Only the 0.29.0 tagged run can show whether that works.

**1. Diff against 3c851646**
```
 scripts/release-linux-binary.adversary-test.sh | 107 +++++++++++++++++++++++++
 1 file changed, 107 insertions(+)
```
I changed only the test file. I added no new script and did not touch `Taskfile.yml`.

**2. Cases added** (section `adversary_w34_u1`, at the end of the script)
- `adversary_w34_u1_sign_fails_when_cosign_fails_late` (2 cases, green, kept). A stub cosign writes the SBOM's bundle and then exits 1; `sign`'s step must fail. The test also makes a copy of the step with `|| true` after `sign-blob`, and the case catches that copy. This kills the surviving `|| true` mutant. Before, the stub only failed without writing a bundle, so `test -s` hid the mutant.
- `adversary_w34_u1_guard_quoted_parens` (red). The guard parser counts parentheses inside string literals. A sign job with `if: ${{ github.event_name != 'pull_request' && '(' || true || ')' }}` is true on a pull request, and `id_token_breach` passes it.
- `adversary_w34_u1_backfill_only_from_main` (red). No job compares `github.ref` with `refs/heads/main`. A `workflow_dispatch` started from another branch or a tag ("Use workflow from") signs as `release.yml@<that ref>`. `publish` uploads those assets, and the upload can never be replaced. Only after the upload does verification fail, because it checks only `refs/heads/main`.
- `adversary_w34_u1_backfill_keeps_the_tag_runs_attestation` (red). Scenario: a tag run gets through `sign` but stops before the upload, and the release is later backfilled. The image tag is kept, so the digest in `IMAGES` is the same, and the image ends up with two attestations: one at the tagged commit, one at main's commit. `all(... == $commit)` then fails the backfill, although D6 A accepts either ref.

These cases cannot run on their own: the script is one file with shared setup. The first time they ran was this whole-script run, and the red output below is verbatim from it.

**3. Suite run** (after the cases existed)
```
$ slot task check:the_release_binary_checks_refuse_what_they_must      exit 201, 10.9 s wall incl. sync
release-linux-binary-adversary: FAIL: adversary_w34_u1_guard_quoted_parens: a copy of release.yml whose sign job runs on a pull request (if: ${{ github.event_name != 'pull_request' && '(' || true || ')' }}) passes adversary_id_token_only_on_signing_jobs
release-linux-binary-adversary: FAIL: adversary_w34_u1_backfill_only_from_main: nothing before sign or image-manifest refuses a workflow_dispatch run whose ref is not refs/heads/main; its assets are signed as release.yml@<that ref>, uploaded, and only then fail publish's verification
release-linux-binary-adversary: FAIL: adversary_w34_u1_backfill_keeps_the_tag_runs_attestation: a backfill fails on the image's attestation from the tag run: $ gh attestation verify oci://ghcr.io/beyond10x/spindle:0.26.0@sha256:9dac…182f --repo beyond10x/spindle --signer-workflow beyond10x/spindle/.github/workflows/release.yml --format json signed by stub at 0000…0007 signed by stub at 0000…0008 The attestation does not name the commit 0000…0008.
release-linux-binary-adversary: 317 cases, 3 failed
```
Where 312 comes from: the implementor's `green.log` on 3c851646. My section adds 5 cases.

**4. Findings that are judgement only** (cover 3c851646)
- `release.yml:924`: `publish` has no `attestations: read`, and the story's D4 forbids adding it. Whether `gh attestation verify` works with this token on a public repo cannot be shown offline. If it fails, the run goes red after an upload that can't be undone. Verdict INFEASIBLE, origin introduced.
- `release.yml:1015`: on a backfill, the attestation is checked against `GITHUB_SHA`, the commit the run itself is on. That check always passes for the run's own attestations, so it shows nothing about the tagged commit. This follows from D6 A and is documented. Verdict INFEASIBLE, origin introduced.
- Verification runs after the upload, and the upload refuses assets that already exist. So if verification fails transiently (TUF, Rekor), re-running `publish` stops at the upload and never verifies again. The story's acceptance asks for this order. Verdict INFEASIBLE, origin introduced.

**5. Attacked without breaking it**
- The cosign 3.1.6 sha256 `318a99ad…` matches the upstream `cosign_checksums.txt`.
- The `attest-build-provenance` pin `4d101475…` is what tag `v4.2.2` points to.
- With `push-to-registry: false`, no storage record is made, so `artifact-metadata: write` is not needed.
- The mutant "`GITHUB_SHA` instead of the tag's commit on a tag push" is equivalent. On a tag push, `github.sha` is the tagged commit, and `provenance` asserts HEAD == `TAG^{commit}`. The surviving mutant cannot be reached.
- The `case` on `IMAGES_EXPECTED` in the upload step, and the byte flip on the changed archive (bytes 0, `%` and `\` included, with a `cmp` guard), hold.

**6. Paths written outside the worktree** (all under `<local path>`)
- `net/cosign_checksums.txt`, `net/action.yml`, `net/README.md`, `net/attest-README.md`, `net/attest-action.yml`: copies of the upstream files I checked the pins against.
- `section.sh`, `new.sh`, `commit-msg.txt`, `run1.log`

Inside the tree, the script writes to `target/release-linux-binary-adversary/adversary-w34-u1/`.

**7. Findings**
```findings
- file: scripts/release-linux-binary.adversary-test.sh
  line: 1248
  category: boundary
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the guard parser counts parentheses inside string literals, so an if true on a pull request ('(' || true || ')') passes adversary_id_token_only_on_signing_jobs
- file: .github/workflows/release.yml
  line: 796
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a workflow_dispatch run from a ref other than refs/heads/main signs under that ref and uploads irreplaceable assets before publish's verification refuses them
- file: .github/workflows/release.yml
  line: 1039
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: a backfill fails because all() demands main's commit while the image digest still carries the tag run's attestation, though D6 A accepts either ref
- file: .github/workflows/release.yml
  line: 924
  category: judgement
  severity: warning
  verdict: INFEASIBLE
  origin: introduced
  message: publish runs gh attestation verify without attestations read, which D4 forbids adding; only the tagged run shows whether the token suffices
- file: .github/workflows/release.yml
  line: 1015
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: on a backfill the attestation is compared with the run's own GITHUB_SHA, which every attestation of that run names, so the commit check proves nothing
- file: .github/workflows/release.yml
  line: 1004
  category: judgement
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: verification runs after an upload that refuses existing assets, so a transient verify failure cannot be retried by re-running publish
```
