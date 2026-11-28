# Wave 34

**Proposed.** Planned on the operator's pick of 2026-11-28 (D-W34 A, relayed by the host operator;
the owner's yes of 11:08 CET for the workflow permissions). All stories serve `vision:spindle`.

Integration branch `wave/34`, forked from `main` after wave 33 (PR 107) lands. Release 0.28.0
(wave 33) comes first; release 0.29.0 follows this wave. This plan was written on `plan/wave-34` at
18715a80 (wave 33's branch, carrying `main` at release 0.27.0); every line number in the stories is
read there.

## Selection

D-W34 A picked three drafts:

- `story:release-artifacts-signed`: no release asset is signed today; a checksum beside an asset
  proves nothing about who built it. Held in wave 33 for the owner's word on the workflow
  permissions `id-token: write` and `attestations: write`, given at 11:08 CET; its
  `needs-owner-acceptance` tag is removed.
- `story:unknown-commit-answered-as-unknown`: a PostgreSQL commit whose outcome is unknown answers
  `501` with `committed: false` today, so a caller sends again a write that may have landed. Found
  by wave 33's U2 adversary.
- `story:reset-on-a-busy-store-keeps-the-store`: a reset held by a busy store loses the
  component's store until the run restarts. Found by wave 33's U2 adversary; its case is parked
  with `#[ignore]` in `adversary_w33_u2.rs`.

Each was scoped by one `story-scoper` (`story:release-artifacts-signed` on 276298bb, its lines read
again on 18715a80), and every claim written into a story was read against the tree at the cited
line. Each story's `## Scope` holds its surface, its `## Decided when scoped` the options taken
(D-W34 A: the scoper's default, option A, of every decision), and its `## Outcome`, `## Why` and
`## Acceptance` were amended where a line was stale or a decision changed it.

Not in the wave; both stories stay as they are:

- `story:sensitive-fields-redacted`: waits on an ESS release with a field sensitivity marker.
- `story:feed-wait-under-load`: option C of D-W34, not picked.

Filed beside the wave, not in it: `story:unknown-commit-in-seed-and-retries` (draft), the seed and
the bound-command retry paths that `story:unknown-commit-answered-as-unknown` leaves out (its D4 A).

Closed with this plan: `story:postgres-lane-margin` and `story:scan-cost-test-steady`, delivered by
PR 106 (merged 8622f0bd), moved to `implemented` with a `test_result` evidence each.

Acceptance amendments, one line per story:

- `story:release-artifacts-signed`: six stub cases in `scripts/release-linux-binary.adversary-test.sh`
  (the permissions on `sign` and `image-manifest` only, a bundle per asset, the uploads, the image
  signed by digest, the pinned tools checked, a backfill verified at `refs/heads/main`); the three real checks (`every_release_asset_verifies`,
  `a_changed_archive_fails_verification`, `the_image_is_signed_when_published`) run in the tagged
  run of release 0.29.0, whose step summary is the story's evidence.
- `story:unknown-commit-answered-as-unknown`: `500` with `{"refused", "committed": "unknown"}` (D1 A,
  D2 A) from a new `StoreError::Unknown` (D3 A), pinned by one kernel unit test, two ingress
  tests and one host test showing that a reset whose removal fails `StoreError::Unknown` still
  loses the store; the two existing before-COMMIT cases pass unchanged.
- `story:reset-on-a-busy-store-keeps-the-store`: the parked case un-ignored; a busy reset keeps the
  store, `503` while no row changed and `500` saying the store is kept after (D1 A); four new host
  and ingress tests, one of them showing that a removal failing `StoreError::Backend` still loses
  the store.

## Units

| unit | story | surface | branch | tree |
|---|---|---|---|---|
| U1 | `story:release-artifacts-signed` | `.github/workflows/release.yml` (new job `sign`, `image-manifest`, `publish`), `scripts/release-linux-binary.adversary-test.sh`, `AGENTS.md` release step 7, `README.md` § Releases | `unit/w34-release-signed` | `spindle-w34-release-signed` |
| U2 | `story:unknown-commit-answered-as-unknown` | `crates/spindle-kernel` (`store.rs` `StoreError`, `sqlite.rs` `backend`), `crates/spindle-ingress/src/answer.rs`, new `tests/spindle-ingress/unknown_commit.rs`, `crates/spindle-host/tests/spindle-host/supplied.rs` (one reset case) | `unit/w34-unknown-commit` | `spindle-w34-unknown-commit` |
| U3 | `story:reset-on-a-busy-store-keeps-the-store` | `crates/spindle-host/src/host.rs` (`Host::empty`, `removed_all`), `crates/spindle-ingress/src/console.rs` (`console::reset`), `crates/spindle-cli/src/serve.rs` (comment only), `adversary_w33_u2.rs`, `busy_store.rs`, `crates/spindle-host/tests/spindle-host/supplied.rs` and `rows.rs` | `unit/w34-busy-reset` | `spindle-w34-busy-reset` |

No `depends_on` edge inside the wave. Each unit is built by one `aep:implementor` and gets one
`aep:adversary` pass when green.

## Surfaces

Files two or more units share, and how they are kept apart:

- `CHANGELOG.md` `[Unreleased]`: all three. U1 under `### Added`, U2 and U3 under `### Fixed`; each
  unit adds its own bullet naming its story id; each merge into `wave/34` keeps both sides, ordered
  by story id. Wave 33's entry naming `story:reset-on-a-busy-store-keeps-the-store` "as before"
  sits under `[0.28.0]` by then and is not edited.
- `website/docs/concepts/capabilities.md`, the store-failure paragraph (`:242-252`): U2 the
  command's `500` with `"committed": "unknown"`, U3 the reset sentence (`:250-252`). Same
  paragraph: U3 merges after U2 and rewrites its sentence on top of U2's.
- `website/docs/concepts/environment-and-admission.md`: U3 (`:363-364`, `:671-673`); U2 only if it
  adds a sentence beside the `503` (`:669`, inferred). Adjacent lines at `:669-673`, kept by
  merging U3 after U2.
- `crates/spindle-ingress/src/answer.rs`: owned by U2 (a new arm for `StoreError::Unknown` beside
  the `ExecuteError::Store` arms, `:584-585`, and the `answer` doc `:145-148`). U3 calls the
  existing `Answer::busy` from `console.rs` and does not edit `answer.rs` (its D1 A); its scope
  entry for the file is inferred, for D1 B only.
- `crates/spindle-kernel/src/store.rs` `StoreError` (`:323`): U2 adds the variant `Unknown`. U3
  does not edit the file; it classifies store errors in `Host::empty`, where only
  `StoreError::Busy` keeps the store, so `StoreError::Unknown` loses it as any other error does.
  The case for that, `a_reset_whose_commit_is_unknown_still_loses_the_store`, is U2's, since U2
  adds the variant.
- `crates/spindle-host/tests/spindle-host/supplied.rs`: U2 adds
  `a_reset_whose_commit_is_unknown_still_loses_the_store`; U3 adds
  `a_reset_whose_first_step_is_busy_keeps_the_supplied_store`,
  `a_busy_store_after_one_that_only_forgot_its_events_is_nothing_reset` and
  `a_reset_whose_store_fails_otherwise_still_loses_it`. Kept apart by distinct case names (and
  stub store names); merged U2, then U3, which keeps both sides.
- `crates/spindle-ingress/tests/spindle-ingress/main.rs`: U2 adds `mod unknown_commit;`. U3 adds
  its ingress case to the existing `busy_store.rs` and does not edit `main.rs`.

No other file is in two units' scope. U1 shares only `CHANGELOG.md`.

## Order and parallel safety

All three are built in parallel from `wave/34`. Merge order: U1, U2, U3. U3 merges after U2 and
resolves the `capabilities.md` paragraph and `supplied.rs` on top of U2's.

- Default test step (`task check:the_tests_pass_within_the_budget`, under 120 s): U2 adds one
  kernel unit test, two ingress tests and one host test (about 1.6 s); U3 un-ignores one ingress case and adds two
  host tests in `supplied.rs`, one in `busy_store.rs` and one more host test (about 2 s); each test
  under 1 s. U1 adds no Rust test; its stub cases run in
  `task check:the_release_binary_checks_refuse_what_they_must`, and the unit names the script's
  time before and after.
- PostgreSQL lane (`task check:postgres`, its parallel part under 100 s; 70.6 s on `wave/33`
  pinned): no unit adds a lane test, so the lane gains 0 s of the 5 s this wave may add.
- U1's real verification runs only in the tagged run of release 0.29.0; its step summary is
  recorded as the story's evidence when the story closes.

## Decisions taken

Every decision takes the scoper's default, option A, each reversible (D-W34 A).

- `story:release-artifacts-signed`: D1 A (`actions/attest-build-provenance` v4.2.2 at
  `4d101475d8b20a2381f78447822ac1eab6504dd8`), D2 A (cosign v3.1.6 release binary,
  `cosign-linux-amd64` sha256 `318a99ade4156a0c4113aa983b46c0a0830fa7896a096c63121ee47b610a94c6`,
  from the version's `cosign_checksums.txt`), D3 A (each archive, the SBOM, `IMAGES` and
  `SHA256SUMS`, one `.sigstore.json` each), D4 A (a new `sign` job before `publish`, never on a pull
  request), D5 A (the image signed in `image-manifest` by its digest), D6 A (backfills signed,
  verified at the tag's ref or `refs/heads/main`), D7 A (stub cosign in the test script, the real
  verification in the tagged run).
- `story:unknown-commit-answered-as-unknown`: D1 A (`500`), D2 A (`{"refused", "committed":
  "unknown"}`), D3 A (`StoreError::Unknown`), D4 A (seed and retries in
  `story:unknown-commit-in-seed-and-retries`).
- `story:reset-on-a-busy-store-keeps-the-store`: D1 A (keep the store; `503` while no row changed,
  else `500` saying it is kept), D2 A (`HostError::Store(StoreError::Busy(..))`, `removed_all`
  typed), D3 A (events forgotten alone count as nothing reset).

## What a unit must not do

- Write Python in anything that stays in the repository: tools, tasks, gates, CI steps and test
  helpers are Rust or POSIX sh.
- Add a test that runs over 60 s: nextest kills it (`.config/nextest.toml`, `slow-timeout` with
  `terminate-after`). A test over 5 s is named with its time in the unit's report.
- Let a test step run over 120 s, or the PostgreSQL lane's parallel part over 100 s.
- Hold a pool job past 10 minutes; a job that needs longer names its time with `slot --timeout`,
  at most 30 minutes. A script that waits or measures starts without the `slot` prefix; `cargo`
  inside it takes a slot through the shim.
- Run the whole workspace's tests; run the tests of the crates the unit touched.
- Make a model call, a call to a real API or a network request in a test; loopback and stub tools
  only. U1's stub cases sign, pull and push nothing.
- Give `id-token: write` or `attestations: write` to any job other than `sign` (both) and
  `image-manifest` (`id-token: write` only), or to a job that runs on a pull request.
- Add a dependency other than U1's two pinned tools (cosign v3.1.6, `actions/attest-build-provenance`
  v4.2.2), each pinned by sha and checked as its decision says.

## Revision round 1

Critics of round 1: scope approved; acceptance, design and parallel safety asked for these.

- U3 no longer depends on U2's variant: its case `a_reset_whose_store_fails_otherwise_still_loses_it` fails a removal with `StoreError::Backend` only, and its Outcome and decisions name no `StoreError::Unknown`; U2 owns the new case `a_reset_whose_commit_is_unknown_still_loses_the_store` (`supplied.rs`), because it adds the variant.
- U1 gains the stub case `adversary_backfill_verifies_at_main`: the verification step passes an identity at `@refs/heads/main` on `workflow_dispatch` and at `@refs/tags/<tag>` on a tag push.
- U1's `adversary_id_token_only_on_signing_jobs` checks that each job holding `id-token: write` has an `if:` false on a `pull_request` event, by a named list of accepted guard forms, not a fixed prefix.
- This page: U3 no longer reads U2's branch before its last round (merge order U1, U2, U3 kept); U2's surface gains `supplied.rs`, shared with U3 by distinct case names; U3's surface gains `serve.rs` (comment only) and `rows.rs`.

## Revision round 2

- The acceptance critic of round 2 found that `a_reset_whose_first_step_is_busy_keeps_the_supplied_store` put a busy `forget_events` on a store holding a row, which `Host::empty` removes first; that leg now runs on a store holding no row (U3).
