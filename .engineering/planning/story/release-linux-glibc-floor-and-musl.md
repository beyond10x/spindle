---
format: aep.planning-md/3
id: story:release-linux-glibc-floor-and-musl
kind: story
status: active
title: The Linux release runs on an old glibc and as a static binary
relations:
- serves: vision:spindle
revision: 16
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T18:58:26Z", actor: "human:operator", revision: 15, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "proposed", to: "active", at: "2026-11-26T18:58:26Z", actor: "human:operator", revision: 16, decided_on: {"recorded":{"review_outcome":14}}}
---
# The Linux release runs on an old glibc and as a static binary



## Outcome

Each release builds its `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` archives against
glibc 2.17, under the asset names they have today, and publishes a new
`x86_64-unknown-linux-musl` static archive beside them. A host on an older glibc (a build node runs
glibc 2.34) runs `b10x-spindle` without the warning or failure a binary linked against a newer glibc
gives it.

Ordered on 2026-11-25 for every Rust tool of the organisation (static musl Linux releases). Refined
on 2026-11-26 from ESS's measurement: musl with mimalloc ran its synthesis workload 82 % slower than
gnu, so ESS builds gnu against glibc 2.17 (cargo-zigbuild, checked with `objdump -T`) and keeps musl
as the static fallback.

## Acceptance

Read on the pull request's run of `release.yml` (the `build` and `sbom` jobs; `publish` is skipped
on a pull request):

- The `build` row `x86_64-unknown-linux-gnu` builds on the glibc 2.17 floor, and a check in that row
  reads the binary's symbol versions with `objdump -T` and fails on any `GLIBC_` above 2.17.
- The same for the `aarch64-unknown-linux-gnu` row.
- A new `build` row `x86_64-unknown-linux-musl` builds; a check in that row confirms the binary is
  static (`file`, `readelf` shows no `INTERP` and no `NEEDED`) and runs `--version` in a pinned
  Alpine image.
- A check runs `--version` of each Linux gnu binary in a `debian:bookworm` image (glibc 2.36).
- Each row's archive is the job artifact named as the release asset will be
  (`b10x-spindle-<v>-<target>.tar.gz`), and the `sbom` job's target list and count check cover the
  five targets.

Read at the next tagged release (the `publish` job):

- The release carries the five archives under those names, and `SHA256SUMS` lists each.

## Joined 2026-11-26

This story takes over story:release-binary-portable-libc, which asked the same for both Linux
targets: the `aarch64-unknown-linux-gnu` archive is built on the same glibc 2.17 floor and checked
the same way, and each Linux archive's `--version` also runs in a `debian:bookworm` container (glibc
2.36). A consumer found 0.21.0 needs `GLIBC_2.39`; an installed binary was still 0.1.0,
which cannot read the specification format consumers write today.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `.github/workflows/release.yml`, jobs `build`, `sbom` and `publish`. Cited: the story's acceptance is about release assets and CI checks.
- **Files:** cited.
  - `release.yml:161-173` is the build matrix. A row for `x86_64-unknown-linux-musl` gets added. The rows `x86_64-unknown-linux-gnu` (`:162`) and `aarch64-unknown-linux-gnu` (`:165`) switch to the 2.17 floor.
  - `:188` sets the rustup targets.
  - `:200-214` downloads cargo-auditable by `${TARGET}` (`:206`) and checks it against one checksum per row (`:164-173`).
  - `:226` runs `cargo auditable build ... --target "$TARGET"`.
  - `:240` runs the `--version` check.
  - `:250` runs `cargo tree --target "$TARGET"`.
  - `:259` requires `eventlog-postgres` in the embedded dependency list.
  - `:264-281` is the Package step.
  - `:328` and `:349` cover the SBOM: the target list and `length == 4`.
  - `:396` and `:402` are the publish `targets` list and the count of files.
  - `:410-413` writes SHA256SUMS.
  - `:434-441` uploads, and refuses to replace an asset that already exists.
- **Symbols:** cited.
  - The matrix keys `target`, `os` and `cargo-auditable-sha256`.
  - The env `TARGET`, `CARGO_AUDITABLE_VERSION`.
  - The SBOM property `cdx:rustc:sbom:target:triple`.
  - The archive name `b10x-spindle-${VERSION}-${TARGET}.tar.gz` (`:270`).
- **Also likely:**
  - New steps in `release.yml`: an `objdump -T` floor check on both gnu rows, and a static check plus an Alpine `--version` run on the musl row. Each Linux archive also gets a `--version` run in `debian:bookworm`. Inferred, from the acceptance and the "Joined 2026-11-26" section.
  - A measurement script, `scripts/measure-release.sh` (no such file today; `scripts/` holds 4 other files). Inferred.
  - A `CHANGELOG.md` entry. Inferred.
- **Documents:** cited.
  - `AGENTS.md:196-211`, release step 6: it names the four targets, the rule "a target is added as a matrix row in the `build` job and a name in the `publish` job's `targets`", and step 7's "one archive per target".
  - `README.md:214-219`, "Releases": "Binaries for Linux and macOS (x86_64 and aarch64 each)".
  - `website/docs/getting-started.md:9-19`: the same sentence, plus "one CycloneDX SBOM ... covering the four targets".
  - The story body itself, which takes the measurement.
- **Not touched:**
  - The `provenance`, `gate` and `ui` jobs (`release.yml:29-151`).
  - `ci.yml`.
  - Every crate's source.
  - `crates/spindle-cli/build.rs`, the UI embedding.
  - The macOS rows (`:168-173`).
  - Inferred: none of these read the target list.
- **Confidence:** high for the workflow and the documents, because the story names the assets and the file holds them all. Medium for where the measurement and allocator code land, because the story names no path.
- **Would collide with:**
  - Any unit editing `.github/workflows/release.yml`, especially the `build` matrix, the SBOM target list or the publish manifest.
  - Any unit editing `AGENTS.md` § Cutting a release, `README.md` § Releases or the intro of `website/docs/getting-started.md`.
  - Cited for the workflow and documents; inferred for the cli crate.
- **Safety fact:** The gnu archive keeps its asset name, so consumers who pin `b10x-spindle-<v>-x86_64-unknown-linux-gnu.tar.gz` get the 2.17 build with no change on their side. The musl archive is a new name, so the duplicate refusal at `:434-440` cannot fire. Level 2 (`release.yml:270`, `:434-440`), unproven.
- **Not established:**
  - Whether `cargo auditable` can wrap `cargo zigbuild`. No sibling combines the two; the sibling repositories ship without cargo-auditable.
  - Whether a `cargo-auditable-x86_64-unknown-linux-musl` asset exists for 0.7.7. `:206` downloads the host tool under the target's name, so a musl row needs either its own checksum or a separate host key.
  - Whether `rustls-native-certs` finds certificates on a bare Alpine. This does not matter for `--version`.

Pattern to copy:
- **ESS (closest fit).** the ESS repository's `.github/workflows/package.yml` on an unmerged branch (not on its `main`).
  - `:91-103`: a matrix with gnu and musl rows for both arches.
  - `:143-152`: `musl-tools` and `CC_<target>=musl-gcc`.
  - `:154-193`: zig and cargo-zigbuild 0.24.1, pinned by checksum for the x86_64 and aarch64 hosts.
  - `:216-223`: per target, `cargo zigbuild --target "$TARGET.2.17"` for gnu and `cargo build` otherwise.
  - `:244-263`: the `objdump -T` floor check.
  - `:265-299`: the static check (`file`, `readelf` INTERP and NEEDED) and a `--version` run in `busybox:1.37.0`, not Alpine.
  - The "shared composite action" the story mentions is ESS's `.github/actions/release-component/action.yml:48-49` (input `glibc-floor`) with its steps at `:138-223`. It installs ESS for a consumer; it does not package another repository. Inferred.
- **A sibling repository** (its `main`), x86_64 only, no Alpine run.
  - a sibling repository's `scripts/package.sh`: pins at `:23-26`, `check_glibc` at `:40`, `check_static` at `:86`, zigbuild for gnu `.2.17` and musl at `:160` and `:166`.
  - `.github/workflows/package.yml:50-54`.
  - a sibling repository's `scripts/measure-release.sh` times gnu, musl and musl-mimalloc round-robin; not used here (see Decided when scoped).
- **Alpine run** to copy: `.github/workflows/release.yml` (`origin/main`).
  - `:117`: `MUSL_FLOOR_IMAGE` is Alpine 3.24.4, pinned by digest.
  - `:1619-1643`: `--version` in that image.
  - `:114-115` and `:1564-1570`: a glibc floor image smoke run, the pattern for the bookworm check.
- **A third sibling repository** (`origin/main`) `.github/workflows/binaries.yml:56-90` uses zigbuild against glibc 2.27 with `an xtask glibc-floor`. Its floor is not 2.17 and it has no musl, so it is the weaker model.
- No sibling repository holds a shared release composite action on its `main`.

Native blockers:
- **libsqlite3-sys 0.39.1, `bundled`.** Chain: rusqlite 0.41.0 ← eventlog-sqlite 0.10.0 ← spindle-kernel default feature `sqlite`. It compiles SQLite C through `cc`. That needs a musl C compiler (`musl-gcc` as ESS does, or zig) and is not a blocker. Cited from `cargo tree -i`.
- **ring 0.17.14**, through rustls, tokio-rustls and tokio-postgres-rustls, all on the `ring` feature (`crates/spindle-cli/Cargo.toml:27,43`, `crates/spindle-kernel/Cargo.toml:25,32`). It builds C and asm through `cc`. Sibling repositories build the same under zig and musl. Not a blocker; inferred.
- **None of these in the `x86_64-unknown-linux-musl` tree of spindle-cli:** openssl-sys, native-tls, aws-lc-rs/aws-lc-sys, mimalloc, zstd-sys, libz-sys. Cited from `cargo tree -i`, which found no match for each.
- **Direct libc calls** are only `prctl` (`crates/spindle-firecracker/src/namespace.rs:97`, `crates/spindle-qemu/src/namespace.rs:79`) and `syscall` (`crates/spindle-qemu/src/bin/init/guest.rs:221`). Both symbols are older than glibc 2.17. Cited.
- **Release must keep postgres in the musl build.** `release.yml:259` requires `eventlog-postgres` in every row's dependency list, and the SBOM check at `:350` requires it too. Cited.

## Decided when scoped

- Build with `cargo zigbuild` for `x86_64-unknown-linux-gnu.2.17`, `aarch64-unknown-linux-gnu.2.17`
  and `x86_64-unknown-linux-musl`; the gnu asset names stay, the musl archive is a new name.
- gnu stays the default Linux archive and musl is the static fallback; no mimalloc. A timing of gnu
  against musl is a benchmark, which the repository's rules keep out of the gate; it is not part of
  this story (Not covered).
- If `cargo auditable` cannot wrap `cargo zigbuild`, the unit reports it and stops; no row ships
  without its embedded dependency list.

## Shared files

- `.github/workflows/release.yml`, `AGENTS.md` § Cutting a release and
  `website/docs/getting-started.md`: this story's alone in wave 26.
- `README.md`: § Releases (`:214-219`) is this story's; U3 (`story:run-fails-on-rejected-line`,
  `story:clock-and-mint-in-every-mode`) edits the run and seed paragraphs (`:56-64`). Separate
  hunks, merged by the coordinator.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.

## Not covered

- A timing of the gnu binary against the musl binary, with or without mimalloc: a benchmark, which
  the repository's rules keep out of the gate. gnu stays the default Linux archive.
- mimalloc.
- An `aarch64-unknown-linux-musl` archive.
