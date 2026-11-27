---
format: aep.planning-md/3
id: story:view-filter-docs-list-every-form
kind: story
status: draft
title: The view filter docs list every quantifier form served
relations:
- serves: vision:spindle
revision: 1
---
## Outcome

`website/docs/concepts/specification.md` and the `unsupported` refusal text in
`crates/spindle-host/src/view.rs` list every quantifier form a view filter is served with: a map
field of the row, a list member of an element, a map member of a list element, and a map of
structs.

## Why

Found by wave 30's U1 adversary (head bae972a3): the code serves a map member of a list element
(`exists v in g.stamps`) and a map of structs (`i.at`, `exists w in i.words`) and answers them as
ESS's interpreter does, but the page and the refusal text name only the first two forms.

## Acceptance

- The page and the refusal text name the four forms; `slot task docs-check` passes.
