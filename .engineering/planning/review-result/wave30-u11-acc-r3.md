---
format: aep.planning-md/3
id: review-result:wave30-u11-acc-r3
kind: review-result
status: active
title: Plan critic (acceptance), round 3, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
needs-revision

story:affects-over-input-list — the HTTP case's fixture needs an "`affects:` entry [that] publishes an event for each row it writes", which ESS cannot express: `RawAffect` refuses unknown fields and holds no `emits:`, and a per-row `emits:` (`each: changed`) is refused on any outcome without `instances:`. The bullet must instead name an event in the outcome's own `emits:`. `/api/history` already answers that event for every row written in the outcome's write group (`Committed::instances`). — .engineering/planning/story/affects-over-input-list.md:84

**What I read:** the one story I was given, read in full, plus the round-1 and round-2 findings and the head commit's diff to the story. Commands: `aep plan artifact show story:affects-over-input-list`, `cat` of the r1 and r2 review results, `git show HEAD`, `aep plan artifact validate` (valid). I checked the U11 parts of `.engineering/waves/wave-30.md` (`:31-32`, `:52`, `:68-102`). I checked these against the tree:
- the fixture's `lot.yaml` and `system.yaml`
- `outcome_scoped_refusal.rs:519-608`
- `console.rs:28,254-267` and `host/range.rs:496-505`
- `store.rs:244-249`

I also read the pinned ESS (d7833ee): `ess-domain` `command/set_effects.rs:40-44,96-136,400-418` and the `wrong_state` row of `ess-conformance` `interpret/execute.rs:16`.

**Earlier findings:**
- **r1 `deletes:` test: resolved.** The test is at `:66-67` and is now also in the Scope list (`:114`).
- **r1 HTTP fixture: resolved.** The HTTP case runs on a new `reached_by: network` fixture (`:82-87`). Its event wording is the new finding above.
- **r1 row state and r2 stdin read-back: resolved.** The stdin case now names `bay.lot.SweepTag` with a `wrong-state` outcome (`:77-81`). ESS answers `wrong_state:` for a held row resting outside the move's `from` states, so a `Swept` tag and a `Live` tag answer differently. The case is writable on the fixture, and the line that selects `swept` changes from `unsupported` to `swept`.

**What I could not establish:**
- Whether an implementer would quietly use the outcome's own event and call the bullet met. As written, the bullet cannot be met.
- Out of my lane (scope): `bay.lot.SweepTag` must also be added to `accepts.commands` in the fixture's `system.yaml` (`:13-17`). The Scope names only `lot.yaml`.
- Out of my lane (scope): the story's status is still `draft`.
- Confirmed: the wave page orders U11 after U5 and U6 (`:97-102`), which matches the story's `Decimal` parity case. The story body names no person, organisation, host or home path.

```findings
- file: .engineering/planning/story/affects-over-input-list.md
  line: 84
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the HTTP case's fixture needs an \"`affects:` entry [that] publishes an event for each row it writes\", which ESS cannot express: `RawAffect` refuses unknown fields and holds no `emits:`, and a per-row `emits:` (`each: changed`) is refused on any outcome without `instances:`. The bullet must instead name an event in the outcome's own `emits:`. `/api/history` already answers that event for every row written in the outcome's write group (`Committed::instances`)."
```
