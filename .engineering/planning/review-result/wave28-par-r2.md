---
format: aep.planning-md/3
id: review-result:wave28-par-r2
kind: review-result
status: active
title: Plan critic (parallel-safety), round 2, on wave 28
relations:
- reviews: story:console-routes-off
- reviews: story:structured-json-logs
- reviews: story:metrics-series-bounded
- reviews: story:seed-readiness
- reviews: story:health-and-graceful-drain
- reviews: story:entity-event-history
- reviews: story:events-record-their-caller
- reviews: story:environment-binds-external-decisions
- reviews: story:replay-with-decisions
- reviews: story:one-time-response-admission
revision: 1
---
needs-revision

- story:replay-with-decisions — its Scope edits the `--decisions` doc comment at `crates/spindle-cli/src/cli.rs:259-265` (**cited**, read in the tree), the same lines `story:environment-binds-external-decisions` (U6) edits as "`--decisions` help" at `:260-265` (**inferred**), and the wave page gives U7 only the `--scenario` conflicts (`:392`) and U6 the `--decisions` help, so a same-line collision (and in generated `website/docs/reference/cli.md`) reads as separate hunks; name U6 and U7 as sharing the `--decisions` help, by an ordering edge (the merge order U7, U6 is already stated) or by moving one story's help text elsewhere — .engineering/waves/wave-28.md:50-52, .engineering/planning/story/replay-with-decisions.md:72, .engineering/planning/story/environment-binds-external-decisions.md:81
- story:events-record-their-caller — it adds `a_seed_event_records_the_seed` to `crates/spindle-cli/tests/spindle-cli/seed.rs` (**inferred** position), while U4 (`story:seed-readiness`, `story:health-and-graceful-drain`) edits that file's `mod` list at `seed.rs:1225-1230`, the last lines of the 1230-line file (**cited**), and the wave page lists the `mod` lines of the `main.rs` test files but not this one, so U4 and U5 share a file the page does not name; add `tests/spindle-cli/seed.rs` to the wave page's shared files with its hunks (U5 test body, U4 `mod` lines) and a merge order, or have U5 put its test in a new module file — .engineering/waves/wave-28.md:82, .engineering/planning/story/events-record-their-caller.md:106, .engineering/planning/story/seed-readiness.md:125

Round-1 findings, all three resolved:
- `router.rs` (U4 and U5): the wave page now lists both units' hunks (`:123-140`, `:342`, `:429` against `:299-346`, `:400-421`) and states "U5 merges before U4".
- `observe.md` (U3 and U4): the wave page names the file, the U3 region `:47-64` and the new U4 section after the metrics section, and states "U3 merges before U4".
- `environment-and-admission.md` (U4 and U6): the wave page names the file, U4 `:153-240` and the U6 section placed after `:240`, and states "U6 merges before U4".

What I read: 10 stories (`aep plan artifact show` for each, `aep plan artifact graph`), `.engineering/waves/wave-28.md`, the round-1 record `r1-par.md`, and the tree at `plan/wave-28` for `cli.rs:115-136,183-194,255-268,385-395`, the tail of `tests/spindle-cli/seed.rs`, and the `Taskfile.yml` and `families.json` generation lines. Surfaces: 10 of 10 stories placed **cited**, 0 placed only by inference, 0 unplaceable. Acceptance reads traced to a producer in the set: 4. Two have a `depends_on` edge: `story:entity-event-history` on `story:events-record-their-caller`, and `story:health-and-graceful-drain` on `story:seed-readiness`. The other two have no edge and the wave page orders them: `story:structured-json-logs`' "every stderr line" reads lines that U1, U3 and U4 add (U2 merges last), and `story:console-routes-off` iterates `Console::ALL`, which U5 grows (U5 merges before U1).

What I could not establish:
- Whether U1's doc-text edit of `with_console` (`surface.rs` about 1505-1511) and U5's `Console::ALL` use inside the same function (`:1517`, `:1626`) merge cleanly. The wave page names both and I did not flag it.
- The wave page lists `crates/spindle-kernel` as "U5 only", but U6 edits `crates/spindle-kernel/tests/spindle-kernel/adversary_environment.rs` (the `Environment` literal). These are different files and no collision, so it is not a finding; the claim is wider than the facts.
- Where in `seed.rs` the test of U5 lands. Hence the second finding rests on an inferred position.
- Out of my lane, not counted: the two U7 stories share `Replays::run` with no `depends_on` edge. The wave page and `story:one-time-response-admission` state the order and they run in one unit.

```findings
- file: .engineering/waves/wave-28.md
  line: 50
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:replay-with-decisions edits the --decisions doc comment at crates/spindle-cli/src/cli.rs:259-265 (cited, replay-with-decisions.md:72), the same lines story:environment-binds-external-decisions edits at 260-265 (inferred, environment-binds-external-decisions.md:81), but the wave page gives U7 only the --scenario conflicts (:392) and U6 the --decisions help, so a same-line collision in cli.rs and generated cli.md reads as separate hunks; name U6 and U7 as sharing the --decisions help, by an ordering edge or by moving one story's help text"
- file: .engineering/planning/story/events-record-their-caller.md
  line: 106
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "adds a_seed_event_records_the_seed to crates/spindle-cli/tests/spindle-cli/seed.rs (inferred position) while U4 edits that file's mod list at seed.rs:1225-1230, the last lines of the file (seed-readiness.md:125, cited); the wave page lists the mod lines of the main.rs test files but not this file, so U4 and U5 share seed.rs unnamed; name the file with each unit's hunk and a merge order, or have U5 put its test in a new module file"
```
