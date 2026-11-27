# Wave 31

**In progress.** Planned by the coordinator on the host operator's order of 2026-11-27, relaying the
operator's words "make sure spindle is moving with its last planned features". Skill
`aep:implementing` 0.26.0, wave mode. All stories serve `vision:spindle`.

Integration branch `wave/31`, forked from `wave/30` after wave 30's units merge. Release 0.26.0
follows this wave.

## Selection

Wave 30 left seven stories that would change the architecture waiting for the operator's pick. On
2026-11-27 the operator picked option A for six of them: `story:verified-caller-token`,
`story:listener-tls`, `story:event-egress-and-dead-letters`, `story:several-writers-one-postgres`,
`story:spec-change-over-stored-rows` and `story:spindle-image-published`. Each was scoped against
`plan/wave-31` at 8eaea47a by one `story-scoper`; each story's `## Scope` holds its surface, its
`## Decided when scoped` the options taken, and its `## Outcome` and `## Acceptance` were amended
where the scoper found the draft stale or a decision changed them.

- `story:event-egress-and-dead-letters` is split (its D1 A) into `story:event-egress-webhook` and
  `story:dead-letters-kept-and-replayed`, two units with no `depends_on` between them. The original
  is `rejected`; its two parts carry the work.
- `story:several-writers-one-postgres` no longer waits for the draft
  `story:request-id-idempotency`: its acceptance line `a_retried_request_lands_once` moved there,
  and the edge became `informed_by` (its D8 A). It now `depends_on
  story:spec-change-over-stored-rows`, the rolling-deploy case that story's check guards.
- `story:postgres-lane-kills-slow-tests` is new: `task check:postgres` runs `cargo test`, so no test
  there is killed at 60 s, and four units of this wave add PostgreSQL tests to it. Found by the
  scoper of `story:several-writers-one-postgres`.

The host operator answered A on 2026-11-27 to the five decisions put to the operator:
`story:verified-caller-token` D1 (`jsonwebtoken` 9.3.1), the split of
`story:event-egress-and-dead-letters` (D1), `story:several-writers-one-postgres` D8,
`story:spec-change-over-stored-rows` D1, `story:spindle-image-published` D1.

Revision round 1 of the plan critics (2026-11-27): `story:verified-caller-token` takes D3 B on the
coordinator's default, reversible: a token run off loopback is served only over the listener's TLS,
so it `depends_on story:listener-tls`. The acceptance lines that read another unit's work became
`depends_on` edges (below), each story that ships a capability lists the generated status page,
and the exit-2 and exit-3 lines name the refusal they read.

## Units

| unit | stories | branch | tree |
|---|---|---|---|
| U1 | `story:postgres-lane-kills-slow-tests` | `unit/w31-pg-lane` | `spindle-w31-pg-lane` |
| U2 | `story:spindle-image-published` | `unit/w31-image` | `spindle-w31-image` |
| U3 | `story:listener-tls` | `unit/w31-tls` | `spindle-w31-tls` |
| U4 | `story:verified-caller-token` | `unit/w31-token` | `spindle-w31-token` |
| U5 | `story:spec-change-over-stored-rows` | `unit/w31-spec-change` | `spindle-w31-spec-change` |
| U6 | `story:several-writers-one-postgres` | `unit/w31-writers` | `spindle-w31-writers` |
| U7 | `story:event-egress-webhook` | `unit/w31-egress` | `spindle-w31-egress` |
| U8 | `story:dead-letters-kept-and-replayed` | `unit/w31-dead-letters` | `spindle-w31-dead-letters` |

`depends_on` edges inside the wave: U4 on U3 (D3 B reads `--tls-cert` and `--tls-key`); U5, U7
and U8 on U1 (their `postgres::` acceptance lines rely on the lane's 60 s kill); U6 on U1, U5, U7
and U8 (its lane, the rolling-deploy check, and `a_standby_posts_no_webhook_and_keeps_no_dead_letter`,
which reads the egress binding and the dead letters). No cycle. Each unit is built by one `aep:implementor` and gets one `aep:adversary` pass
when green. Every brief names the 60 s bound per test, the 120 s step budget and the 10-minute
pool limit; the PostgreSQL tests run in the `task check:postgres` lane.

## Surfaces

Each story's `## Scope` holds its surface. Shared between units, each on its own hunk unless said,
and resolved by the coordinator at merge:

- `crates/spindle-cli/src/serve.rs`: U3 a `Serving` field beside `proxy_secret` (`:128-133`) and the
  TLS constructor at `bind_reading_on` (`:498`); U4 `Serving` fields, the surface construction
  (`:372-389`) and the startup lines (`:394-405`, `:436-445`); U5 the exit-3 mapping of a misfit
  (`:337`, `:468-474`); U6 `serve` (`:301`): the lease before `Host::with_stores` (`:463`), a
  listener that answers before the host exists (`:498`), the renew; U7 the egress thread beside the
  served run; U8 the failure loop (`:612-650`).
- `crates/spindle-cli/src/cli.rs` and the generated `website/docs/reference/cli.md`: U3 three flags
  in `Command::Run` beside `--address` (`:280-295`); U4 the token flag beside
  `--trusted-caller-header` (`:342-368`) and `console_served` (`:766-771`); U5 exit 3 of `run`; U6
  the `run` doc on readiness (`:155-170`); U7 the help of `run`.
- `crates/spindle-cli/src/main.rs`: U3 the `Command::Run` destructure (`:213-258`), `Execute`
  (`:588-622`), the early refusals of `execute` (`:647-677`) and the `Serving` literal
  (`:759-781`); U4 the same destructure, `Execute` and its flag checks (`:595-676`) and the ingress
  refusals (`:817-823`); U5 the stdin `run` SQLite store (`:859-871`); U7 the egress bind beside
  `Calls::bind` (`:713`).
- `crates/spindle-cli/src/calls.rs`: U4 makes `client`, `file_roots` and `platform_roots`
  reachable for the JWKS fetch, with a caller's label in `file_roots`' error (`:386-419`); U7 reuses
  `Call::send` and the same client.
- `crates/spindle-cli/src/database.rs`: U5 `stores` (`:129-134`); U6 `stores` (`:101`), `migrate`
  (`:139`), a `database_lease` setting (`:38`) and the connection budget (`:103-130`). One function.
- `crates/spindle-ingress/src/listener.rs`: U3 `Listener::serve` (`:380-460`), `connection`
  (`:530-550`), a constructor beside `bind_reading_on` (`:366-378`); U4 `answer` (`:581-705`) and
  `proven` (`:749-767`), with its unrecorded `401` beside the secret refusal (`:597-606`); U6 a
  standby flag beside `stopping` (`:154`, `:174`, `:494`), the standby's `503` in `answer` beside
  the `stopping` check (`:609`, next to U4's hunk), and `bind_reading_on` with `serve`
  (`:366-400`), which take a `Reader` today and must bind before the host exists, where U3 adds its
  constructor.
- `crates/spindle-docs/src/status.rs` with the generated `website/data/status.json` and
  `website/docs/status.md`: U3, U4, U5, U7 and U8 each a row for their capability; U6 the
  `planned` row "Cells, ownership and snapshots" (`:533-538`). The generated pages also carry the
  "N shipped, M planned" count, so every merge conflicts there: the coordinator merges `status.rs`
  by hand and regenerates both pages (`task docs-generate`) after each of these merges.
- `crates/spindle-ingress/src/lib.rs`: U3 the `listener` re-export (`:93`) and the module doc
  (`:31-41`); U4 a re-export (`:94-96`). `Cargo.lock`: U3 `spindle-ingress`'s dependency list, U4
  `jsonwebtoken`. `website/docs/concepts/capabilities.md` `:115-125`: U3 and U4, one paragraph.
- `crates/spindle-kernel/src/sqlite.rs`: U5, U7 and U8 each a reserved `spindle:log` stream beside
  `MARKS` (`:93-108`) and its skip in `groups_after` (`:1351`), one hunk; U6 the fence in
  `commit_all` (`:579`, `append_group` `:659`) and `append` (`:846`, `:855`).
- `crates/spindle-kernel/src/postgres.rs`: U5 at most a doc comment of `open`; U6 `migrate`, `open`
  and `impl Log`.
- `crates/spindle-kernel/src/store.rs` and `contract.rs`: U7 the cursor's `Store` methods, U8 the
  dead letters'. `crates/spindle-kernel/src/lib.rs`: U5 `mod fit`, U7 and U8 re-exports.
- `crates/spindle-host/src/host.rs`: U5 the SQLite open in `Host::assemble` (`:1121-1127`) and a
  `HostError` variant (`:257`); U6 the doc comments at `:535-537` and `:1681-1684`; U8 `Failed`
  (`:77-97`) and `Host::attempt` (`:2242-2380`).
- The observe model and its generated types (`ess/domains/observe.yaml`, `ess/generated/jsonschema`,
  `crates/spindle-observe-api`, `ui/src/api/generated`): U8 `DeadLetter` beside `RefusedBound`
  (`:825`); U6 `Readiness.standby` (`:926`). Each regenerates the whole set.
- `Taskfile.yml`: U1 `check:postgres` (`:178-189`) and `test:slowest` (`:68-71`); U2 the `desc` of
  `check:the_release_binary_checks_refuse_what_they_must` (`:49-52`); U8 `OBSERVE_ROOTS` (`:7`).
- `docs/design/spindle.md`: U4 a row after `:427` in § 11; U6 the row at `:429`, `:336-338` and M5
  (`:457`); U7 and U8 § 11.
- `website/docs/concepts/environment-and-admission.md`: U5 "State in PostgreSQL" (`:372`); U6 the
  seed on replicas (`:262`) and the lease grants (`:395-458`); U7 the `[egress]` table.
- `README.md`: U2 § Releases (`:257-264`); U7 egress; U8 the console route list.
- The test module lists `main.rs` of `spindle-cli` (U3, U4, U5, U7, U8) and `spindle-kernel` (U5,
  U7, U8); `spindle-cli` `postgres.rs` (U5, U6, U7); `spindle-kernel` `postgres.rs` (U5, U6, U8);
  `spindle-kernel` `log_streams.rs` (U7, U8); `CHANGELOG.md` `[Unreleased]` (every unit but U1).

Wave 30's U9 and U12 change `crates/spindle-cli/src/cli.rs`, `main.rs` and `serve.rs` (U12 also
`spindle-host` `host.rs`, `spindle-ingress` `surface.rs`, `console.rs`, `router.rs` and `lib.rs`,
`capabilities.md` and `README.md`) before wave 31 forks, so the line numbers above, taken on
`plan/wave-31` before wave 30 merged, move. Wave 30's other overlaps move lines only, as wave 31
forks after they merge: `spindle-host` `host.rs` (wave 30 U1, U6, U7; wave 31 U5, U6, U8),
`spindle-kernel` `lib.rs` (wave 30 U5, U6; wave 31 U5, U7, U8), the test `main.rs` lists of
`spindle-kernel` (wave 30 U3, U6, U11; wave 31 U5, U7, U8), `spindle-ingress` (wave 30 U3; wave 31
U3) and `spindle-host` (wave 30 U1, U8; wave 31 U8), `README.md` (wave 30 U3; wave 31 U2, U7, U8),
and `console.rs` and `router.rs` (wave 30 U12's `Console` and `answer`; wave 31 U8's two routes).

Merge order: U1, U2, U3, U5, U7, U4, U8, U6.

- U1 first: the PostgreSQL lane kills a test at 60 s before U5, U6, U7 and U8 add tests to it
  (each `depends_on` U1). Its `Taskfile.yml` hunks are apart from U2's and U8's.
- U2 next: no crate change; it shares only `Taskfile.yml`, `README.md` § Releases and
  `CHANGELOG.md`.
- U3 before U4: U4 `depends_on` U3 (D3 B); the same `listener.rs`, the same `run` flags in `cli.rs`, `Execute` and its early
  refusals in `main.rs`, `Serving` in `serve.rs`, `spindle-ingress` `lib.rs`, `capabilities.md` and
  `Cargo.lock`.
- U5 before U7 and U8: it adds the first new reserved stream and its skip in `groups_after`
  (`sqlite.rs:1351`), the hunk U7 and U8 extend; and before U6, which `depends_on` it and rewrites
  `database::stores` after it.
- U7 before U4: U4 reshapes the `calls.rs` TLS helpers for its JWKS fetch once U7's use of them is
  on `wave/31`.
- U8 after U7: the same reserved streams, `store.rs`, `contract.rs`, the kernel's `lib.rs`,
  `log_streams.rs`, `serve.rs`, `README.md` and design § 11.
- U6 last: after U1, U5, U7 and U8 (`depends_on` each); its fence in the `sqlite.rs` commit path and its lease at the
  start of `serve` land after every other change to those files; its `Readiness.standby`
  regenerates the observe types after U8's `DeadLetter`; and its acceptance line
  `a_standby_posts_no_webhook_and_keeps_no_dead_letter` needs U7 and U8. The coordinator merges
  `wave/31` into U6's branch before its last run.

## Exit

Each story moves `active -> implemented` on a `test_result` (the PR's CI run, the `PostgreSQL` job
included for U1, U5, U6, U7 and U8) and, where the adversary found something, a `review_outcome`.
U2 closes on the stub cases of that run; its `IMAGES` asset and manifest list are verified in the
0.26.0 release run as a step of `AGENTS.md` § Cutting a release, not as part of this exit.

## Commits

The unit commits, the merges into `wave/31`, review fix commits, the closing store commit, one PR to
`main` through the bot, and release 0.26.0 on the host operator's word after the PR merges. U2's
first push to ghcr.io happens in that release's run.
