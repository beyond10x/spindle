---
format: aep.planning-md/3
id: story:spindle-image-published
kind: story
status: proposed
title: The release publishes a Spindle image with its digest
relations:
- serves: vision:spindle
- decomposes: epic:kubernetes-target
- depends_on: story:ingress-bind-address
- informed_by: story:release-binary-portable-libc
scope:
- confidence: cited
  path: .github/workflows/release.yml
- confidence: inferred
  path: AGENTS.md
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Containerfile
- confidence: cited
  path: README.md
- confidence: inferred
  path: Taskfile.yml
- confidence: inferred
  path: scripts/release-linux-binary.adversary-test.sh
- confidence: inferred
  path: scripts/release-linux-binary.sh
- confidence: cited
  path: website/docs/getting-started.md
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-27T09:56:33Z", actor: "human:operator", revision: 6}
---
# The release publishes a Spindle image with its digest

## Outcome

Each release builds and pushes `ghcr.io/beyond10x/spindle:<version>`, one manifest list for
`linux/amd64` and `linux/arm64`, holding the `b10x-spindle` binary the release already builds for
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` (`.github/workflows/release.yml:163-168`),
on a minimal base, as a non-root user, with `b10x-spindle` as the entrypoint. The manifest list's
digest is attached to the release as `IMAGES` (`<image>@sha256:<digest>`) beside `SHA256SUMS`
(`release.yml:477-498`, uploaded at `:513-527`) and written to the release job's summary; the
workflow does not edit the release notes (D1 A).

## Acceptance

- The release job fails when the image's `b10x-spindle --version` does not print the release's
  version, on either architecture (`adversary_image_version_refuses_another_version`).
- The release's assets hold `IMAGES`, and the digest in it resolves to a manifest list with both
  architectures (`adversary_manifest_list_needs_both_architectures`,
  `adversary_images_line_is_pinned`).
- The image's configured user is not root (UID other than 0) (`adversary_image_user_refuses_root`;
  the `Containerfile`'s `FROM` pinned by digest, `USER` numeric and not 0, `ENTRYPOINT`
  `b10x-spindle`, no `RUN`: `adversary_containerfile_is_pinned_nonroot_copy_only`).
- With `story:ingress-bind-address`'s option, `run` in the container, given `--address` off
  loopback with `--trusted-caller-header` and `--trusted-caller-secret`, answers a request sent
  with the secret from outside the container to the published port
  (`adversary_image_answers_off_container`).

The named cases are POSIX sh cases on stub tools in
`scripts/release-linux-binary.adversary-test.sh`, run by
`check:the_release_binary_checks_refuse_what_they_must`; the real checks run in the `Release`
workflow's image jobs.

The story closes on the stub cases: it moves `active -> implemented` on a `test_result` of the
pull request's CI run, where those cases pass and the image jobs build and check the image on both
architectures without pushing (D3 A). The `IMAGES` asset and its manifest list are verified in the
0.26.0 release run, as a step of `AGENTS.md` § Cutting a release, not as part of the wave's exit.

## Scope

Derived 2026-11-27 by `story-scoper` on `plan/wave-31` at 8eaea47a. Every line is **cited** (read
from the story or the tree) or **inferred** (a reading that could be wrong). Option A (distroless/cc,
gnu binaries, linux/amd64 + linux/arm64, push to ghcr.io with `packages: write`) is scoped; B, C, D
are not.

- **Primary surface:** `.github/workflows/release.yml`; no crate changes — cited
- **Files:** `.github/workflows/release.yml:13-16` (`pull_request.paths`: add the container file),
  `:18-19` (top-level `contents: read`; `packages: write` goes on the push job only), `:154-375`
  (`build`; the gnu rows `:163-168` on `ubuntu-24.04` / `ubuntu-24.04-arm` hand their archives to
  the new image jobs through `:368-375`), `:452-527` (`publish`: `needs`, the file count at `:487`,
  `SHA256SUMS` `:494-498`, the upload list `:519` and `:526`) — cited
- **Files:** `Containerfile` (new, repository root): `FROM` distroless/cc pinned by digest, `COPY`
  of the extracted binary, numeric non-root `USER`, `ENTRYPOINT ["/b10x-spindle"]`, no `RUN` —
  inferred (the story says "for example `Containerfile`")
- **Files:** `README.md:257-264` (`## Releases`) and `website/docs/getting-started.md:9-12` (the
  releases paragraph) — cited
- **Files:** `scripts/release-linux-binary.sh` (new modes: version and user of an image with no
  shell, platforms of a manifest list, the `IMAGES` line; `runs-in` at `:97-120` needs `sh` and `ldd`
  in the image and cannot check distroless) — inferred
- **Files:** `scripts/release-linux-binary.adversary-test.sh` (stub cases for those modes; the
  target-list check at `:215-225` must accept the image job's two-arch list) — inferred
- **Symbols:** jobs `build`, `publish`; new jobs `image` (matrix over the two gnu rows) and
  `image-manifest`; env `GLIBC_FLOOR`, `GLIBC_IMAGE` — cited / new names inferred
- **Also likely:** `AGENTS.md:170-218` (§ Cutting a release, steps 6-7: the image, `IMAGES`, the
  verification), `CHANGELOG.md` `[Unreleased]`, `Taskfile.yml:49-52` (the `desc` of
  `check:the_release_binary_checks_refuse_what_they_must`) — inferred
- **Documents:** `README.md`, `website/docs/getting-started.md`, `AGENTS.md` — cited / inferred as
  above. Constraint: `website/ux/e2e/findings.spec.ts` UXF-007 wants exactly one releases link on
  Getting started, and UXF-018 refuses any `b10x-spindle X.Y.Z` that is not the latest
  `CHANGELOG.md` release, so the page names the image as `ghcr.io/beyond10x/spindle:<version>` —
  cited
- **Confidence:** high for `release.yml`, `Containerfile`, `README.md` and Getting started (the story
  and tree name them); medium for the script and adversary test (one way to make the checks
  testable)
- **Would collide with:** any unit touching `.github/workflows/release.yml` (notably
  `story:console-image-published`, which depends on this one), `scripts/release-linux-binary*.sh`,
  `README.md` § Releases, `AGENTS.md` § Cutting a release, `CHANGELOG.md` `[Unreleased]`
- **Safety fact:** nothing reaches ghcr.io for an unverified commit: the push job `needs`
  `provenance` and `gate` and carries `if: github.event_name != 'pull_request'`, as `publish` does
  at `release.yml:452-455`, and checks version, user and answer on each arch before any push —
  level 2, unproven

## Notes

`story:console-image-published` edits the same workflow; they do not run beside each other.

## Consumer gap

Recorded 2026-11-24. A downstream consumer running a specification on 0.19.1 in a container has
no published image: every user writes a Dockerfile that downloads the release archive and checks
it against `SHA256SUMS`. This story is that gap; its acceptance asks for a versioned image with its
digest in the release's `IMAGES` asset (D1 A; corrected 2026-11-27: the text said the release
notes).

The image's base must run the binary the release builds. Since 0.22.0
(`story:release-linux-glibc-floor-and-musl`) the Linux binaries need glibc 2.17, so a Debian
bookworm or distroless base runs them, and a static `x86_64-unknown-linux-musl` archive runs on a
base with no libc (`scratch`). Corrected 2026-11-27: the text said `GLIBC_2.39`, the floor before
0.22.0.

## Architecture decision

Option A, picked by the operator on 2026-11-27: a distroless image with glibc
(`distroless/cc`) built from the gnu binaries for `linux/amd64` and `linux/arm64`, a new
Containerfile, and a release job that pushes to `ghcr.io` (`packages: write`). The operator
approved the first push.

Not taken: B (a static image from musl, which needs a new aarch64 musl target), C (a static amd64
image only), D (defer).

## Decided when scoped

Taken 2026-11-27 on the scoper's recommended option, each reversible.

- D1 A: the digest is shown in the `IMAGES` asset and the job summary; the workflow still never
  edits the release page, and `AGENTS.md` step 7 verifies the asset; answered A by the host operator on 2026-11-27, and this
  default stands. Not taken: B, `publish` edits the notes with `gh release edit`; C,
  a manual bot step after the run.
- D2 A: one image job per architecture on native runners builds, loads and checks version, user
  and answer before it pushes; a merge job makes the manifest list `<version>` and confirms both
  platforms.
- D3 A: pull-request runs build and check the image on both architectures and push nothing.
- D4 A: the tag is `<version>` only; a tag already in ghcr.io is refused, not retagged.
- D5 A: `IMAGES` is listed in `SHA256SUMS`.
- D6 A: the base is `gcr.io/distroless/cc-debian12:nonroot` (glibc 2.36), pinned by digest.
