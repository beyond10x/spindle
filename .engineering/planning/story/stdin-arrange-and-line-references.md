---
format: aep.planning-md/3
id: story:stdin-arrange-and-line-references
kind: story
status: draft
title: A stdin run arranges rows and names what earlier lines minted
relations:
- serves: vision:spindle
revision: 2
---
# A stdin run arranges rows and names what earlier lines minted

## Outcome

A stdin run takes two more kinds of line beside a command:

- an arrange line that writes stored rows before the next command, as a scenario's `arrange:` and
  `POST /api/reset` `rows` do;
- a command line whose input names an identity an earlier line minted, or an event an earlier line
  published, by a reference to that line's answer.

Today a stdin line is a command only. A consumer feeds Spindle one line at a time over pipes and
resolves references itself from earlier answers, and cannot set up rows at all on stdin.
`--scenario` has `arrange:` and `capture:`, but ESS's scenario rules refuse acts sent as a system
actor, which stdin accepts.

## Acceptance

- `an_arrange_line_writes_rows_a_later_command_reads`: an arrange line with one row, then a command
  guarded on that row, answers the guarded outcome.
- `a_line_names_an_identity_an_earlier_line_minted`: a create line, then an update line whose
  instance is a reference to the create's minted identity, answers the update's outcome.
- `a_reference_to_a_line_not_yet_answered_is_rejected`: a reference to a later line, an unknown
  line or a field the answer lacks rejects the line, and with story:run-fails-on-rejected-line the
  run exits 1.
- `an_arrange_line_refused_by_the_schema_is_rejected`: a row whose fields the entity does not
  declare rejects the line and writes nothing.
- `a_system_actor_command_on_stdin_still_answers`: a command granted only to a system actor, sent
  on stdin, answers as today.

## Not covered

- References across separate runs.
- Changing ESS's scenario authoring rules.

## Origin

A consumer's simulation, 2026-11-26, on 0.21.0 (`run --help`: a stdin line is a command only).
