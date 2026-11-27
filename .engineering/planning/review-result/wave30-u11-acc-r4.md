---
format: aep.planning-md/3
id: review-result:wave30-u11-acc-r4
kind: review-result
status: active
title: Plan critic (acceptance), round 4, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
approve

The round-3 blocker is fixed, the two earlier findings stay fixed, and I found nothing new in the acceptance of `story:affects-over-input-list`.

**What I read:** the one story I was given, `story:affects-over-input-list` (revision 16), read in full.
- **Commands:** `aep plan artifact show story:affects-over-input-list`, `cat` of `wave30-u11-acc-r1` to `-r3`, `git show HEAD` (01169b71) and `aep plan artifact validate`, which answered `valid`.
- **Tree:** the U11 lines of `.engineering/waves/wave-30.md` (`:31-32`, `:52`, `:68-102`), `store.rs:238-262` (`Committed::instances`, `instances_of`), `spindle-host/src/host/range.rs:500-538` (`history`) and `console.rs:28,254-277`.

**Earlier findings:**
- **r1 `deletes:` test: still fixed.** The story names `an_affects_entry_over_an_input_list_deletes_only_held_rows`.
- **r1 HTTP fixture: still fixed.** The HTTP case runs on the new `affects-each-network` fixture, which is `reached_by: network`.
- **r1 row state and r2 stdin read-back: still fixed.** The stdin case reads each tag's state back through the new `bay.lot.SweepTag` command and its `wrong-state` outcome. `SweepTag` is now also in the Scope's `accepts.commands` of `system.yaml`.
- **r3 HTTP event: fixed.** The bullet now names an event in the outcome's own `emits:`. The tree supports the check: `range.rs:500,538` filters `/api/history` by the instances a write wrote, and `Committed::instances` lists every instance of the write group (`store.rs:244-249`). So the history of each created or moved row answers the outcome's event, and before this change the command was refused with a 501, so no such event existed.

**What I could not establish:**
- Out of my lane (scope): no acceptance line covers the `CHANGELOG.md` `[Unreleased]` entry the Scope names.
- Out of my lane (scope): the story's status is still `draft`.
- Confirmed: the wave page orders U11 after U5 and U6 and merges `wave/30` into U11's branch before its last run (`:97-102`), as the `Decimal` parity case needs. The story body names no person, organisation, host or home path.

```findings
[]
```
