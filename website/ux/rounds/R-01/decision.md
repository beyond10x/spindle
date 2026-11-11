# R-01 decision: add an offline full-text search to the navbar?

Carried from R-00 (`../R-00/decision.md`, unanswered there; its default was yes).

Finding: UXF-001. Target metric: gate-2 "search" (always visible), now absent on 48 of 48 gating
checks, expected 0; `screen_only.actions` of `http-call` (24 in R-01) and `environment` (6) should
fall, because the screen-only reader types the task's first word into the field when nothing on
screen carries a task word.

Yes (option A): `@easyops-cn/docusaurus-search-local` 0.55.3, a theme of `website/docusaurus.config.ts`
with a local lunr index built with the site (`hashed`, English, docs only, highlight on the target
page). No hosted service, no key, no request leaves the site. At 997 px and wider a field after the
theme toggle; below, a 44 px field beside the menu button that widens on focus.
Shots: ../R-00/shots/decision-yes--desktop.png, ../R-00/shots/decision-yes--phone.png (mock-ups);
the built field: after/shots/contact--1280x800--light.jpg, after/shots/contact--390x844--light.jpg.

No: the navbar stays as it is; the `search` mark of UXF-001 stays in `known-failing.json`.
Shots: ../R-00/shots/decision-no--desktop.png, ../R-00/shots/decision-no--phone.png.

Default if unanswered by the next round: yes, because three of five R-00 tasks fail or cost 12 to
26 actions for want of a way to look a term up, and an offline index adds no service and no key.

Answer: yes (default A), 2026-11-10

Bot change that comes with it (decision id `R-01`): the screen-only reader uses the search field
when no control on screen carries any task word and hidden navigation has been revealed and
closed without one, once per play, before it scrolls. It clicks the field (one action), types the
first word (one action) and reads the results as controls of role `option`, skipping a result on
the page it is on; with no result it presses Escape (one action). By keyboard it reaches a result
with the arrow keys, counted with the Tabs. A task value that moves because of this policy is
taken into the baseline under `R-01`.
