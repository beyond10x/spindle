# R-00 decision: add an offline full-text search to the navbar?

Finding: UXF-001. Target metric: gate-2 "search" (always visible), now absent on 48 of 48 gating
checks, expected 0; with it, `screen_only.actions` of `http`, `observe` and `release` (now 19, 12,
26 in R-00) should fall, because the screen-only reader types the task's first word into the search
field (a bot change that comes with the search, measured in the round after it lands).

Yes: a search field in the navbar from a local-index plugin (for example
`@easyops-cn/docusaurus-search-local`): the index is built with the site, served from GitHub Pages,
no hosted service, no keys, no request leaves the site. At 997 px and wider a field after the theme
toggle; below, an icon button beside the menu button. One new dependency in `website/package.json`
(or, better for every beyond10x site, in `@beyond10x/docs-system`'s product layer).
Shots (mock-ups, the field drawn into the navbar's empty search slot):
shots/decision-yes--desktop.png, shots/decision-yes--phone.png

No: the navbar stays as it is; readers find terms through the sidebar links of this round's change
set and their browser's find-in-page. The `search` mark of UXF-001 stays in
`ux/known-failing.json`. Shots: shots/decision-no--desktop.png, shots/decision-no--phone.png
(the site after the R-00 change set).

Default if unanswered by the next round: yes, because three of five R-00 tasks fail or cost 12 to
26 actions for want of a way to look a term up, and an offline index adds no service and no key.

Answer:
