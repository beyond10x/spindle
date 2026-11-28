---
format: aep.planning-md/3
id: review-result:wave31-adversary-U2-r2
kind: review-result
status: active
title: Adversary, round 2, on wave 31 U2
relations:
- reviews: story:spindle-image-published
revision: 1
---
unit: story:spindle-image-published (wave 31 U2), round 2; the findings cover implementor head 47984e70; my cases are committed on top as 93341910 on unit/w31-image
verdict: NEEDS-CHANGE
cases: executed 228→242, red 3
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 13 paths from round 1, plus 6 new in round 2 (part 6)
needs-coordinator: none

**1. Diff stat** (47984e70..HEAD). The only path is a test file.
```
 scripts/release-linux-binary.adversary-test.sh | 99 ++++++++++++++++++++++++++
 1 file changed, 99 insertions(+)
```

**2. Cases added**

They are in the section "adversary_w31_U2, round 2" of `<local path>`.

The first run was `slot sh scripts/release-linux-binary.adversary-test.sh`. It showed the three red cases below, then stopped on a quoting fault in my third-entry case (`sed: unknown option to 's'`, a `/` inside the pattern), and a `cmd && fail` at the end of a loop also tripped `set -e`. Both faults were mine, not findings; I fixed them, and the rerun showed `242 cases, 3 failed`.

Red now:
- **CR continuation:** `containerfile` passes `LABEL a=b \` followed by a carriage return. BuildKit removes the line's `\r\n` before it looks for the escape, so this is still a continuation line. Output: `not refused (exit 0): …/Containerfile: one FROM pinned by digest, …`
- **Containerfile moved:** the provenance step "Whether the commit builds an image" runs for a tag push of 0.26.0 (`GITHUB_EVENT_NAME=push`, `EVENT=push`, `VERSION=0.26.0`) on a commit whose Containerfile is in `deploy/`. Output: `a tag push of 0.26.0 whose Containerfile moved to deploy/ answers containerfile=false, so the release publishes without IMAGES`
- **Loose "not found" match:** in the `image-manifest` tag step, the inspect fails with `ERROR: error getting credentials - err: exec: "docker-credential-ghcr": executable file not found in $PATH`. Output: `buildx imagetools inspect ghcr.io/beyond10x/spindle:0.26.0;buildx imagetools create --tag …`, so the step tags anyway.

Green now:
- The `if` of `image-push` and of `image-manifest` holds no `always()`, `cancelled()` or `failure()`, so the implicit `success()` stays in force. Mutant m6 adds `!cancelled() &&` to both. The unit's cases stay at 0 red; my 2 cases go red. With that mutant, only a missing `image-checked-*` artifact would stop a push after a failed gate.
- An existing tag that holds the two checked images plus a third entry is refused, and nothing is created or written to `IMAGES`. I tried three third entries: one with no platform, one `unknown/unknown`, and a duplicate of the amd64 entry (9 cases).

**3. Suite run** (after the cases existed)
- `slot task check:the_release_binary_checks_refuse_what_they_must` exited 201. Output: `release-linux-binary-adversary: 242 cases, 3 failed`, then `task: Failed to run task … exit status 1`. Wall time 6.8 s.
- Before count, the implementor's file at 47984e70: `228 cases, 0 failed`.
- All 9 of my round-1 cases now pass.

**4. Findings**
- **`.github/workflows/release.yml:83`, an image-less release from a moved Containerfile.** `containerfile=false` comes only from `[ -f Containerfile ]` at the repository root. Moving or renaming the file makes a new release skip `image`, `image-push` and `image-manifest`, and `publish` then uploads green without `IMAGES`. That contradicts the acceptance "the release's assets hold `IMAGES`". A pull request doing the move also stays green, because its image job is skipped.
  - What reaches it: a change that moves or renames the Containerfile, followed by a release tag.
  - Fix: answer `false` only for a version older than the first image release (0.26.0), or only on `workflow_dispatch`; refuse it on a tag push.
- **`.github/workflows/release.yml:729`, the "not found" match is loose.** `*"not found"*` accepts any error text that contains those words, such as a credential helper missing from PATH.
  - What reaches it: nothing I could show tags wrongly. In my example `create` would fail on the same credential error. INFEASIBLE.
  - Fix: match the resolver's form, `*"${IMAGE}:${VERSION}: not found"`.
- **`scripts/release-linux-binary.sh:210`, CR before the continuation.** Only trailing spaces are stripped; a carriage return after the backslash still passes. As in round 1, I found no way to misuse it with CI's one-file build context.

**5. Attacked and not broken**
- `publish` with gate, build or sbom failed, or with `image-manifest` failed or skipped while a Containerfile is present (the `if` requires each).
- `image-push` on a pull request, and on `workflow_dispatch` without a valid annotated tag (provenance fails first).
- An existing tag with the right digests on swapped platforms (refused; the unit's own case "2 1" covers it).
- A third entry or a duplicate platform, in both modes of `manifest-list`.
- `copy_mode` with `4555`, `1555`, `04555`, `u+s`, `+t`, `0o4555`, two `--chmod` flags, or none.
- `images-line` with an empty version or a version that is not `<major>.<minor>.<patch>`.
- A missing Containerfile in a subdirectory does not mislead the check (it reads the root path from the workspace root); a move to a subdirectory does, which is finding 1.

Not attacked: `--chmod=` used only as a decoy in a COPY destination path. It cannot put a binary at `/b10x-spindle` with the one-file build context.

**6. Paths written outside the worktree**, all under `<local path>`:
- Round 2: `r2-first-run.log`, `r2-second-run.log`, `r2-suite.log`, `commit-msg-r2.txt`, `mut/m6.yml`, `mut/m6.yml.log`
- Round 1, still there: `first-run.log`, `suite.log`, `commit-msg.txt`, `mut/m1.sh` to `mut/m5.yml` and their `.log` files

I made a temporary `new.sh` there and deleted it. Inside the worktree, in the git-ignored `target/` (on the build node's copy): `target/adv-r2-before.sh`.

**7.**
```findings
- file: .github/workflows/release.yml
  line: 83
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: containerfile=false comes from a root-path existence test alone, so a tag push of a new version whose Containerfile moved or was renamed publishes green with no image and no IMAGES
- file: .github/workflows/release.yml
  line: 729
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the tag step reads any inspect error containing "not found", such as a missing credential helper, as the tag's absence and runs imagetools create
- file: scripts/release-linux-binary.sh
  line: 210
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a backslash followed by a carriage return passes the no-continuation check, while BuildKit drops the CR and reads a continuation line
```
