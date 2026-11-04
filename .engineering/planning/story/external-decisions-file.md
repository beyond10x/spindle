---
format: aep.planning-md/3
id: story:external-decisions-file
kind: story
status: active
title: External decisions from a file
relations:
- serves: vision:spindle
- decomposes: epic:m0-run-in-process
- depends_on: story:execute-commands
revision: 3
transitions:
- {from: "draft", to: "proposed", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-04T10:45:53Z", actor: "human:timo", revision: 3}
---
# External decisions from a file

## Outcome

`run` takes `--decisions <file>` (outcome name → answer, optionally per command) and `--answer-default`, which takes each command's default branch, so non-interactive runs need no per-line `external` answers.

## Acceptance

- `a_decisions_file_answers_every_external_outcome`
- `answer_default_takes_the_default_branch`
- `an_unanswered_external_without_a_default_fails_the_line`
