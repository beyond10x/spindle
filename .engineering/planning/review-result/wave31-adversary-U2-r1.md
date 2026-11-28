---
format: aep.planning-md/3
id: review-result:wave31-adversary-U2-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U2
relations:
- reviews: story:spindle-image-published
revision: 1
---
unit: story:spindle-image-published (wave 31 U2); the findings cover implementor head babf1807; my cases are committed on top as ac947936 on unit/w31-image
verdict: NEEDS-CHANGE
cases: executed 172→190, red 9
origin: introduced 7 / pre-existing 0 / undecided 0
wrote-outside-worktree: 13 paths, all under the assigned adv-scratch directory (part 6)
needs-coordinator: none

**1. Diff stat** (babf1807..HEAD). The only path is a test file.
```
 scripts/release-linux-binary.adversary-test.sh | 114 +++++++++++++++++++++++++
 1 file changed, 114 insertions(+)
```

**2. Cases added**

They are in the section `adversary_w31_U2` of `<local path>`, starting at line 638. They were written before anything ran. The first run of the script was `slot sh scripts/release-linux-binary.adversary-test.sh`, which ended `190 cases, 9 failed` with exit 1. The script cannot run one case alone, so that first run included the whole suite with my cases already in it.

Red now:
- **setuid/setgid binary (4 cases):** `containerfile` passes `COPY --chmod=4555`, `2555`, `6555` and `u+s`. Output: `not refused (exit 0): …/Containerfile: one FROM pinned by digest, COPY only, a numeric user other than 0, ENTRYPOINT b10x-spindle`
- **Continuation with trailing whitespace (2 cases):** `containerfile` passes `LABEL a=b \` followed by a space or a tab. Docker reads both as a continuation line. Same pass line as above.
- **Third manifest entry:** `manifest-list` passes a third entry that has a `sha512:` digest and no platform. Output: `…: a manifest list of linux/amd64 and linux/arm64`
- **Empty version:** `images-line` with an empty version passes `ghcr.io/beyond10x/spindle:@sha256:<hex>`.
- **Tag check fails open:** the `image-manifest` step that refuses an existing tag was pulled out of `release.yml` and run under bash with a docker stub. When the inspect fails with a timeout rather than "not found", the step runs `buildx imagetools create --tag ghcr.io/beyond10x/spindle:0.26.0 …` anyway.

Green now. Each one is the only case that catches a mutant the unit's own cases miss. I ran each mutant on a copy in scratch through `RELEASE_LINUX_BINARY` / `RELEASE_WORKFLOW`; the unit's cases stayed at 0 red every time:
| Mutant | Case that catches it |
|---|---|
| m1: FROM's space check removed | `FROM --platform=linux/amd64 <pinned>` |
| m2: `*:*:*` removed from `numeric_user` | `USER` / image user `65532:65532:65532` and `65532:65532:0` (4 cases red) |
| m3: gate clause dropped from the image job's `if` | the gate-success case |
| m5: a skipped gate accepted on any event | the skipped-gate pairing case |
| m4: the tag-exists block deleted from `image-manifest` | the "tag exists" run of the step |

A control case also stays green: when the tag is not found, the step does call `create`.

**3. Suite run** (after the cases existed)
- `slot task check:the_release_binary_checks_refuse_what_they_must` exited 201. Output: `release-linux-binary-adversary: 190 cases, 9 failed`, then `task: Failed to run task … exit status 1`. Wall time 5.4 s.
- Before count: `slot sh -c 'git show HEAD:scripts/release-linux-binary.adversary-test.sh > target/adv-unit-only.sh && sh target/adv-unit-only.sh'` → `172 cases, 0 failed`. HEAD here is the implementor's babf1807, before my commit, so this run excludes my section.
- Run on this host directly, the script counts 191 cases. The extra one is a host-binary glibc case that runs only where objdump is installed.
- The mutant runs go without `slot`: slot refuses a file under scratch and says a probe that compiles nothing runs directly. Each mutant run takes about 5 s.

**4. Findings**
- **`scripts/release-linux-binary.sh:206` (`COPY)`), setuid/setgid root.** `containerfile` refuses nothing in `--chmod`. COPY makes the file root-owned by default and `docker run` does not set no-new-privileges, so a setuid binary runs as UID 0 under `USER 65532`. `image-user` still passes, because it only reads `Config.User`.
  - What reaches it: an edit of the Containerfile's `--chmod`, which is exactly what this check exists to guard. The committed file uses 0555, so nothing reaches it today.
  - Fix: refuse a mode with a 4, 2 or 6 in the thousands digit, or a symbolic mode with `s`.
- **`scripts/release-linux-binary.sh:176`, trailing whitespace after a continuation.** The check catches a backslash only as the last byte of the line. BuildKit treats a backslash followed by spaces or tabs as a continuation too, so the script's own claim of "no continuation line" is false.
  - What reaches it: no misuse I could find. In CI the build context holds only the binary, and every joined pair of allowed lines fails Docker's parse.
  - Fix: strip trailing blanks before the check.
- **`scripts/release-linux-binary.sh:335`, counting entries.** `manifest-list` counts entries by `"digest":"sha256:…"`, so an entry with a different digest algorithm is not counted.
  - What reaches it: nothing. `imagetools create` from two sha256 digests never writes such an entry. INFEASIBLE.
- **`scripts/release-linux-binary.sh:355`, empty version.** `images-line` accepts an empty version.
  - What reaches it: nothing. Provenance checks the version against its regex, and `imagetools create --tag "${IMAGE}:"` fails first. INFEASIBLE.
- **`.github/workflows/release.yml:623`, the tag check fails open.** Any failure of the inspect is read as "the tag is absent", and the step then tags. That breaks decision D4, "a tag already in ghcr.io is refused, not retagged", whenever the registry times out, returns a 5xx or rate-limits the inspect, and the create that follows succeeds.
  - Fix: tag only when the inspect's stderr says `not found`; refuse on any other failure. The image job has already pushed by digest by then, so the package exists and a missing tag returns 404.
- **`.github/workflows/release.yml:654`, backfill (judgement, no case).** `publish` now `needs: image-manifest`, and `image-manifest` refuses an existing tag. `AGENTS.md:231` still says "`workflow_dispatch` with `tag` backfills an existing tag", which no longer holds in two situations:
  - A release whose `publish` failed after `image-manifest` had tagged, then re-dispatched.
  - Any tag older than this change. The image job checks out the tag's commit, which has no Containerfile and no `containerfile` mode.
  - "Re-run failed jobs" on `publish` still works.
- **`.github/workflows/release.yml:465`, token scope on pull requests (judgement, no case).** The `image` job holds `packages: write` on pull-request runs from same-repository branches, while it runs the pull request's own scripts. The push steps are guarded, and fork pull requests get a read-only token anyway. The scope note asked for `packages: write` "on the push job only".

**5. Attacked and not broken**
- `numeric_user` on these spellings: `0`, `00`, `0:0`, `65532:0`, `root`, an empty user, `:x`, `x:`, `-1`, a leading space.
- Containerfile parser directives in any key spelling, `RUN`/`ADD`/`ONBUILD`/`SHELL`, lowercase instructions, `COPY --from`.
- `image-version` when the version is only on the second line.
- `image-answers`: the secret header against `surface.rs:1210`, and `/__spindle/` refusing a request without the secret (`status.rs:453`).
- `manifest-list` with a duplicate platform or a third platform.
- `images-line` with an uppercase digest, CRLF line endings, or two lines.
- Every image step that logs in or pushes is guarded on pull requests; `image-manifest` and `publish` keep the implicit `success()`.

The real image (distroless, `--read-only`, running on arm64) was not run: the brief allows docker only where the existing cases use it, and they use none.

I took no worktree lease.

**6. Paths written outside the worktree**, all under `<local path>`:
- `first-run.log`
- `suite.log`
- `commit-msg.txt`
- `mut/m1.sh`, `mut/m2.sh`, `mut/m3.yml`, `mut/m4.yml`, `mut/m5.yml`
- `mut/m1.sh.log`, `mut/m2.sh.log`, `mut/m3.yml.log`, `mut/m4.yml.log`, `mut/m5.yml.log`

Inside the worktree, in the git-ignored `target/` (locally and on the build node's copy): `target/release-linux-binary-adversary/` and `target/adv-unit-only.sh`.

**7.**
```findings
- file: scripts/release-linux-binary.sh
  line: 206
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: containerfile passes COPY --chmod with a setuid or setgid mode, so a root-owned entrypoint runs as UID 0 under USER 65532 while image-user still passes
- file: scripts/release-linux-binary.sh
  line: 176
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a backslash followed by spaces or tabs is a continuation line to BuildKit but passes the check that promises no continuation line
- file: scripts/release-linux-binary.sh
  line: 335
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: manifest-list counts entries by sha256 digest only, so a third entry with another digest algorithm and no platform passes as a two-platform list
- file: scripts/release-linux-binary.sh
  line: 355
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: images-line accepts an empty version and passes repository:@sha256:digest, which names no tag
- file: .github/workflows/release.yml
  line: 623
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: the tag-exists check treats any inspect failure as absent and tags, so a registry error lets image-manifest move a released tag against D4
- file: .github/workflows/release.yml
  line: 654
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: publish now needs image-manifest, which refuses an existing tag and needs the Containerfile at the tag's commit, so the workflow_dispatch backfill AGENTS.md:231 promises no longer uploads for a re-dispatched or pre-image tag
- file: .github/workflows/release.yml
  line: 465
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the image job holds packages write on pull-request runs from same-repository branches while running the pull request's scripts, wider than the scope's push-job-only grant
```
