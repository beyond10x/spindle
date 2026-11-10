# R-01: operator notes

- Base: the round measures `origin/main` at 82faeb87 (wave 18) with R-00's uncommitted change set on
  top. Wave 18 added the section "An external cause bound to an HTTP call" to Environment and
  admission (+307 words) and the topology to the `--observe` row of the generated CLI reference.
  Their reading-load values are worse than R-00's baseline because of content main added, not a UI
  change; they are taken into the baseline under the decision id `R-01-base-82faeb87`.
- Design decision pending from R-00 (UXF-001, offline search): the operator's default is yes,
  option A, an offline search plugin with a local index and no external service. It is this
  round's decision (`decision.md`).
- Cards: anchors A-newcomer and B-integrator; R-01 rotates R01-caller in (tasks `http-call` and
  `topology`). R00-operator's task `observe` stays as a regression scenario.
- Tool changes before this measurement: the `last` column reads the previous round's last state
  (`R-00/after/`); a page key `layout_shift.score` (the sum of layout shifts without input from
  load to the page's end, tolerance 0.01); trend columns for tasks, glance cost and layout shift.
- People: none. The cards are played by the screen-only reader (L3). No recordings (no rrweb
  recorder in this harness); the evidence is the page checks, the plays and the shots.
- Not run: L4 and L5 (paid, no keys), L6 and L8 (no people).
