---
format: aep.planning-md/3
id: review-result:wave30-u11-acc-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on wave 30 (U11)
relations:
- reviews: story:affects-over-input-list
revision: 1
---
needs-revision

story:affects-over-input-list — the stdin acceptance says each tag's state is "read back by a following line", but `run` on stdin reads only command and `arrange` lines, and no command in the `outcome-scoped-refusal-effects` fixture answers differently for a `Swept` tag than for a `Live` one, so the read-back cannot be written on the fixture as named; the bullet must name the line that reads it (for example a new `bay.lot.SweepTag` in `lot.yaml` whose `wrong-state` outcome answers for a swept tag) — .engineering/planning/story/affects-over-input-list.md:77-79

**What I read:** the one story I was given, through `aep plan artifact show story:affects-over-input-list` and the file. I also read the round-1 findings (`aep plan artifact show review-result:wave30-u11-acc-r1`), the head commit `32764572` (`git show HEAD`) with its changes to the story and to `.engineering/waves/wave-30.md` (units table, Surfaces, merge order), and `aep plan artifact validate` (valid). I checked these against the tree and ESS:
- `outcome_scoped_refusal.rs:519-608` and the fixture `lot.yaml` and `system.yaml`
- the `run` and `admit` sections of `website/docs/reference/cli.md`
- `console.rs` and `Host::history` for `/api/history`, plus the matching `CHANGELOG.md` entry
- `supported.rs:587-591` (`compensates`), the `adversary-outcome-scoped-refusal-compensates` fixture and `specification.md:118-128`
- ESS `set_effects.rs` at d7833ee (`elements`, `affect_effect`)
- `story:decimal-identity-any-spelling` (U6), for the `Decimal` parity case

**Round-1 findings:**
- **`deletes:` test (blocker): resolved.** `an_affects_entry_over_an_input_list_deletes_only_held_rows` is at `:66-67`.
- **HTTP fixture (blocker): resolved.** The HTTP case is now its own test, `http_serves_an_affects_entry_over_an_input_list`, on a new `reached_by: network` fixture at `:80-84`. It reads each row back through `/api/history`, which exists in the tree (`console.rs:58`).
- **Row state (warning): partly resolved.** Both cases now name the state they check: the stdin case says each listed tag rests in `Swept` and every other tag stays where it was, and the HTTP case names each row's write. The stdin read-back has no means on the fixture, which is the finding above. ESS skips a held row that is outside the move's `from` states (`set_effects.rs:131-133`, `:232-233`), so a second `CloseLot` that sweeps again would not reveal a tag's state either.

**What I could not establish:**
- `/api/history` returns events, not writes (`Host::history` reads `events_after`). The HTTP case can only be checked if the new fixture's outcome publishes an event; the story does not say so. I list this as an unease, not a finding, because the fixture is new and the implementer writes it.
- Out of my lane (scope): the Scope lines `:111-112` are stale against the revised Acceptance. `:112` still names `run_answers_an_affects_entry_over_an_input_list_on_stdin_and_http` in `outcome_scoped_refusal.rs`, and `:111` leaves out the `deletes:` test.
- Out of my lane (scope): the story's status is still `draft` (`:5`), while the other stories of the wave are `active`.
- Confirmed: U6's Outcome says ESS treats `1.0` and `1` as one row, so the `Decimal` parity case can pass once U6 is merged in. The wave page orders U11 after U5 and U6 with `wave/30` merged in before its last run, which matches the story. `compensates: true` is still refused by the kernel (`supported.rs:590-591`). The story body names no person, organisation, host or home path.

```findings
- file: .engineering/planning/story/affects-over-input-list.md
  line: 77
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the stdin acceptance says each tag's state is \"read back by a following line\", but `run` on stdin reads only command and `arrange` lines, and no command in the `outcome-scoped-refusal-effects` fixture answers differently for a `Swept` tag than for a `Live` one, so the read-back cannot be written on the fixture as named; the bullet must name the line that reads it (for example a new `bay.lot.SweepTag` in `lot.yaml` whose `wrong-state` outcome answers for a swept tag)"
```
