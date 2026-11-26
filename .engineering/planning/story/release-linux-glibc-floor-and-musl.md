---
format: aep.planning-md/3
id: story:release-linux-glibc-floor-and-musl
kind: story
status: draft
title: The Linux release runs on an old glibc and as a static binary
relations:
- serves: vision:spindle
revision: 5
---
# The Linux release runs on an old glibc and as a static binary

## Outcome

Each release publishes, beside today's archives, an `x86_64-unknown-linux-gnu` binary built against
glibc 2.17 and an `x86_64-unknown-linux-musl` static archive. A host on an older glibc (a build node
runs glibc 2.34) runs `b10x-spindle` without the warning or failure a binary linked against a newer
glibc gives it.

Ordered on 2026-11-25 for every beyond10x Rust tool (static musl Linux releases, mimalloc where
allocation is heavy, consumers pin the musl archives). Refined on 2026-11-26 from ESS's
measurement: musl with mimalloc ran its synthesis workload 82 % slower than gnu, so ESS builds gnu
against glibc 2.17 (cargo-zigbuild, checked with `objdump -T`) and keeps musl as the static
fallback; the shared composite action takes the 2.17 floor as an option.

## Acceptance

- The release workflow uploads `b10x-spindle-<v>-x86_64-unknown-linux-gnu` built on the glibc 2.17
  floor, and a CI check reads its symbol versions with `objdump -T` and fails on any `GLIBC_` above
  2.17.
- It uploads `b10x-spindle-<v>-x86_64-unknown-linux-musl.tar.gz`; a CI check confirms the binary is
  static and runs `--version` in a plain Alpine container.
- One Spindle workload (a served specification under a fixed request run) is timed on the gnu and
  the musl binary side by side, with and without mimalloc; mimalloc is kept only where it is not
  slower, and the measurement is written in this story before musl is named a default anywhere.
- `SHA256SUMS` covers the new archives.

## Joined 2026-11-26

This story takes over story:release-binary-portable-libc, which asked the same for both Linux
targets: the `aarch64-unknown-linux-gnu` archive is built on the same glibc 2.17 floor and checked
the same way, and each Linux archive's `--version` also runs in a `debian:bookworm` container (glibc
2.36). A consumer found 0.21.0 needs `GLIBC_2.39`; the binary installed on this host was still 0.1.0,
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
  - If mimalloc is kept: a `mimalloc` feature in `crates/spindle-cli/Cargo.toml` and a `#[global_allocator]` in `crates/spindle-cli/src/main.rs`. The tree has none today (`git grep global_allocator` is empty). Inferred.
  - A `CHANGELOG.md` entry. Inferred.
- **Documents:** cited.
  - `AGENTS.md:196-211`, release step 6: it names the four targets, the rule "a target is added as a matrix row in the `build` job and a name in the `publish` job's `targets`", and step 7's "one archive per target".
  - `README.md:214-219`, "Releases": "Binaries for Linux and macOS (x86_64 and aarch64 each)".
  - `website/docs/getting-started.md:9-19`: the same sentence, plus "one CycloneDX SBOM ... covering the four targets".
  - The story body itself, which takes the measurement.
- **Not touched:**
  - The `provenance`, `gate` and `ui` jobs (`release.yml:29-151`).
  - `ci.yml`.
  - Every crate's source, unless mimalloc is kept.
  - `crates/spindle-cli/build.rs`, the UI embedding.
  - The macOS rows (`:168-173`).
  - Inferred: none of these read the target list.
- **Confidence:** high for the workflow and the documents, because the story names the assets and the file holds them all. Medium for where the measurement and allocator code land, because the story names no path.
- **Would collide with:**
  - Any unit editing `.github/workflows/release.yml`, especially the `build` matrix, the SBOM target list or the publish manifest.
  - Any unit editing `AGENTS.md` § Cutting a release, `README.md` § Releases or the intro of `website/docs/getting-started.md`.
  - `crates/spindle-cli/Cargo.toml` and `src/main.rs`, if mimalloc is added.
  - Cited for the workflow and documents; inferred for the cli crate.
- **Safety fact:** The gnu archive keeps its asset name, so consumers who pin `b10x-spindle-<v>-x86_64-unknown-linux-gnu.tar.gz` get the 2.17 build with no change on their side. The musl archive is a new name, so the duplicate refusal at `:434-440` cannot fire. Level 2 (`release.yml:270`, `:434-440`), unproven.
- **Not established:**
  - Whether `cargo auditable` can wrap `cargo zigbuild`. No sibling combines the two; canon, ESS and substrate ship without cargo-auditable.
  - Whether a `cargo-auditable-x86_64-unknown-linux-musl` asset exists for 0.7.7. `:206` downloads the host tool under the target's name, so a musl row needs either its own checksum or a separate host key.
  - Whether `rustls-native-certs` finds certificates on a bare Alpine. This does not matter for `--version`.

Pattern to copy:
- **ESS (closest fit).** ESS (branch `issues/open-2026-11-26-e`) .github/workflows/package.yml`. It is on branch `issues/open-2026-11-26-e` and not on ESS `origin/main` (a861afd90e, where `git grep zigbuild` is empty).
  - `:91-103`: a matrix with gnu and musl rows for both arches.
  - `:143-152`: `musl-tools` and `CC_<target>=musl-gcc`.
  - `:154-193`: zig and cargo-zigbuild 0.24.1, pinned by checksum for the x86_64 and aarch64 hosts.
  - `:216-223`: per target, `cargo zigbuild --target "$TARGET.2.17"` for gnu and `cargo build` otherwise.
  - `:244-263`: the `objdump -T` floor check.
  - `:265-299`: the static check (`file`, `readelf` INTERP and NEEDED) and a `--version` run in `busybox:1.37.0`, not Alpine.
  - The "shared composite action" the story mentions is `.../ess-issues-1126e/.github/actions/release-component/action.yml:48-49` (input `glibc-floor`) with its steps at `:138-223`. It installs ESS for a consumer; it does not package another repository. Inferred.
- **canon** (`origin/main` 6c1a7fa069), x86_64 only, no Alpine run.
  - `canon/scripts/package.sh`: pins at `:23-26`, `check_glibc` at `:40`, `check_static` at `:86`, zigbuild for gnu `.2.17` and musl at `:160` and `:166`.
  - `.github/workflows/package.yml:50-54`.
  - `canon/scripts/measure-release.sh` is the model for the measurement acceptance: gnu, musl and musl-mimalloc run round-robin, with the decision rule at `:191-197`.
- **Alpine run** to copy: `substrate/.github/workflows/release.yml` (`origin/main`).
  - `:117`: `MUSL_FLOOR_IMAGE` is Alpine 3.24.4, pinned by digest.
  - `:1619-1643`: `--version` in that image.
  - `:114-115` and `:1564-1570`: a glibc floor image smoke run, the pattern for the bookworm check.
- **loom** (`origin/main`) `.github/workflows/binaries.yml:56-90` uses zigbuild against glibc 2.27 with `loom-xtask glibc-floor`. Its floor is not 2.17 and it has no musl, so it is the weaker model.
- **gates** has no shared release composite action on `origin/main`.

Native blockers:
- **libsqlite3-sys 0.39.1, `bundled`.** Chain: rusqlite 0.41.0 ← eventlog-sqlite 0.10.0 ← spindle-kernel default feature `sqlite`. It compiles SQLite C through `cc`. That needs a musl C compiler (`musl-gcc` as ESS does, or zig) and is not a blocker. Cited from `cargo tree -i`.
- **ring 0.17.14**, through rustls, tokio-rustls and tokio-postgres-rustls, all on the `ring` feature (`crates/spindle-cli/Cargo.toml:27,43`, `crates/spindle-kernel/Cargo.toml:25,32`). It builds C and asm through `cc`. canon and ESS build the same under zig and musl. Not a blocker; inferred.
- **None of these in the `x86_64-unknown-linux-musl` tree of spindle-cli:** openssl-sys, native-tls, aws-lc-rs/aws-lc-sys, mimalloc, zstd-sys, libz-sys. Cited from `cargo tree -i`, which found no match for each.
- **Direct libc calls** are only `prctl` (`crates/spindle-firecracker/src/namespace.rs:97`, `crates/spindle-qemu/src/namespace.rs:79`) and `syscall` (`crates/spindle-qemu/src/bin/init/guest.rs:221`). Both symbols are older than glibc 2.17. Cited.
- **Release must keep postgres in the musl build.** `release.yml:259` requires `eventlog-postgres` in every row's dependency list, and the SBOM check at `:350` requires it too. Cited.

## Decided when scoped

- Build with `cargo zigbuild` for `x86_64-unknown-linux-gnu.2.17`, `aarch64-unknown-linux-gnu.2.17`
  and `x86_64-unknown-linux-musl`; the gnu asset names stay, the musl archive is a new name.
- Checks: `objdump -T` floor on both gnu rows; `file`/`readelf` static check and `--version` in a
  pinned Alpine image on the musl row; `--version` of each Linux archive in `debian:bookworm`.
- The SBOM target list, the count checks and the publish target list grow from 4 to 5.
- If `cargo auditable` cannot wrap `cargo zigbuild`, the unit reports it and stops; no row ships
  without its embedded dependency list.
- The musl-versus-gnu measurement follows the round-robin form of a sibling repository's
  `scripts/measure-release.sh`; musl becomes the default only on that measurement.

## Shared files

- `.github/workflows/release.yml`, `AGENTS.md` § Cutting a release, `README.md` § Releases and
  `website/docs/getting-started.md`: this story's alone in wave 26.
- `crates/spindle-cli/Cargo.toml` and `src/main.rs` only if mimalloc is measured into the build;
  `main.rs` is then shared with U3, a separate hunk (`#[global_allocator]`).
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.
