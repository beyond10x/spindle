---
format: aep.planning-md/3
id: review-result:wave26-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 26
relations:
- reviews: story:related-selector-rows-quantifier
- reviews: story:guard-over-absent-row-field
- reviews: story:view-filter-parameter-literal
- reviews: story:run-fails-on-rejected-line
- reviews: story:clock-and-mint-in-every-mode
- reviews: story:scenario-act-without-outcome
- reviews: story:scenario-without-components
- reviews: story:release-linux-glibc-floor-and-musl
revision: 1
---
needs-revision

Findings (all `warning`):

- story:view-filter-parameter-literal — the Outcome says the consumer's `501`s are "on nine routes, and seven more routes", but the report lists nine view routes with 0 journeys (conversations, events, campaigns, group agents, calls, short callflows, simple/dialer, simple/inbound, simple/outbound), so "seven more" is wrong — `.engineering/planning/story/view-filter-parameter-literal.md:32`, `<local path>`
- story:run-fails-on-rejected-line — the Outcome claims "Reported 2026-11-26 by two consumers driving Spindle 0.21.0 from scripts", but neither report in the brief contains a stdin-line or seed-entry exit status. The only nearby sentence is gap 10g, about `--scenario` exit codes, and gap 8 shows exit 0 for a refused act, which the scenario story already treats as ESS parity. The body must cite its source report or say the order "resolve all missing spindle features" is its only origin — `.engineering/planning/story/run-fails-on-rejected-line.md:25`, `<local path>`
- story:clock-and-mint-in-every-mode — gap 7 names an inconsistency: `admit` answers "admitted on sim / identity.mint = seeded" and then `run` refuses the same binding, and gap 6 notes the environment `clock = [...]` binding is not checked. The story adds only flags. It does not say what the `seeded` and `fixed` environment bindings do afterwards, and no Not covered entry names them — `<local path>`
- story:clock-and-mint-in-every-mode — gap 6 asks for "a settable or stepping clock for `run` (stdin and scenario)". The story steps the clock only through `"at"` on a stdin line. A `--scenario` run and an HTTP run get a start instant only. The scenario `at:` stepping (gap 10f, "tied to gap 6") is neither claimed nor listed as not covered — `<local path>`
- story:scenario-act-without-outcome — gap 8 wants an act without an outcome to print "what happened (outcome, error, events), and a flag to continue after a divergence". The story claims only the observed outcome in the printed line. The error and events, the continue-after-divergence flag, and the replay stopping at the first divergence are neither claimed nor recorded as not covered (the story has no Not covered section) — `<local path>`

What I read: 8 stories with `aep plan artifact show` and `aep plan artifact graph`. I also read the wave page, the three reports named in the task, the `story:unknown-instance-after-exists-false-guard` archive (its "Not a gap" section has the reason), `story:stdin-arrange-and-line-references`, the three `joined-*.md` session reports, and `git diff` of the worktree.

What I could not establish:
- Parent promises: the set has no epic parent, so I took the promises from the three listed reports and the order "resolve all missing spindle features". I extracted 25 (view-filter report: 18 filtered views and 2 non-view ingress refusals, counted as 3 groups; a consumer's gaps 1-9 and 10a-g: 22). I traced 20 to a story in this set, wave 27, or a recorded decision:
  - Gaps 2, 5, 10a, 10d, 10e, 10f and 10g are recorded decisions: archived as not a gap, or a note that the guard already names `defined(<field>)`.
  - Gaps 9b, 10b and 10c go to wave 27 in `story:stdin-arrange-and-line-references`.
  - The two non-view ingress refusals (caller attribute, query-string input) go to wave 27 as drafts still to be written.
- The three working-tree edits are in progress: `clock-and-mint-in-every-mode.md`, `scenario-without-components.md` and `release-linux-glibc-floor-and-musl.md`. They were being changed while I read, so I judged the working-tree text for the third and the committed text for the others. The release story no longer names a host, a local tree or a sibling repository, so I raise no leak finding there. It lost its H1 line in that edit.
- Out of my lane, not counted in the verdict: the clock story's Outcome still says periodic bindings fire, and the scenario-act Outcome still says "exits 1", while "Decided when scoped" overrides both. Neither story has a `## Not covered` heading. The acceptance critic or the design critic should check those.

```findings
- file: .engineering/planning/story/view-filter-parameter-literal.md
  line: 32
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the Outcome says 'nine routes, and seven more routes' but the consumer's report lists nine view routes with 0 journeys, so 'seven more' is wrong"
- file: .engineering/planning/story/run-fails-on-rejected-line.md
  line: 25
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the body claims two consumers reported a stdin-line and seed-entry exit status, but neither listed report contains it (gap 10g is about --scenario exit codes), so the body must cite its source or say it rests on the operator's order alone"
- file: <local path>
  line: 83
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:clock-and-mint-in-every-mode neither claims nor lists as not covered the reported admit/run inconsistency on the seeded identity.mint binding and the unchecked clock environment binding"
- file: <local path>
  line: 77
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:clock-and-mint-in-every-mode narrows 'a settable or stepping clock for run (stdin and scenario)' to stepping on stdin lines only, with scenario and HTTP stepping neither claimed nor named as not covered"
- file: <local path>
  line: 92
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "story:scenario-act-without-outcome claims only the observed outcome in the printed line; the reported wants for error and events output and a continue-after-divergence flag are neither claimed nor named as not covered"
```
