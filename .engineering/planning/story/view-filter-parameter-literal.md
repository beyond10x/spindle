---
format: aep.planning-md/3
id: story:view-filter-parameter-literal
kind: story
status: active
title: A view filter reads every parameter form a consumer's model declares
relations:
- serves: vision:spindle
- informed_by: story:view-filter-text-contains
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T18:58:23Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":14}}}
- {from: "proposed", to: "active", at: "2026-11-26T18:58:24Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":14}}}
---
# A view filter reads every parameter form a consumer's model declares

## Outcome

A view filter that reads a parameter in any of the forms below, combined by `any`, `all` and `not`
with the leaves read today, is read as ESS 0.99.1's interpreter reads it, and the view answers the
same rows instead of being refused whole (`crates/spindle-host/src/view.rs`, `unsupported`,
`reads`):

- a parameter compared with a literal of its type: `param.label == ""`, `param.has_agent == false`,
  `param.type == all`;
- a text parameter under a text operation: `param.open in_ignore_case ["on", "true"]`,
  `p equals_ignore_case "inbound"` for an element `p` of a list parameter, `trimmed(...)` around
  such a test;
- a parameter cast in a comparison: `created >= param.time_start as timestamp`;
- the count of a list parameter: `param.type.count == 0`;
- a member of an element of a list field compared with a parameter:
  `exists g in groups: (g.id == param.id)`.

Reported 2026-11-26 by a consumer on Spindle 0.21.0: 205 of its journeys stop at a list route
answering `501` with `unsupported: view … declares the filter …`, on nine routes, and nine more
routes answer the same with no journey reaching them first. Each filter there is built of the forms
above and of leaves Spindle already reads.

## Acceptance

One test per form, each comparing the view's rows with `ess_conformance::interpret` row by row,
with the parameter left out and given:

- F1a `param.<text> == ""`; F1b `param.<boolean> == false`; F1c `param.<enum> == <word>`.
- F2a `param.<text> in_ignore_case [...]`; F2b `param.<text> equals_ignore_case "…"`;
  F2c `p equals_ignore_case "…"` and `p in_ignore_case [...]` for an element `p` of a list
  parameter; F2d `trimmed(...)` around F2a and around F2c.
- F3 `<timestamp field> >= param.<x> as timestamp`: admitted today, a regression test.
- F4 `param.<list>.count == 0`.
- F5a `exists g in <list field>: (g.<member> == param.<x>)`; F5b the same compared with an outer
  list-parameter binder (`r.m == g`); F5c `defined(<binder>.<member>)` for an element of a list
  field.
- F6 a row field under a case fold, `<field> equals_ignore_case "…"`.

And:

- `the_consumer_filter_shapes_are_served`: three views built of the forms together, in the shape of
  the consumer's filters (optional-parameter guards `not defined(param.x) or …`, an `in_ignore_case`
  switch over a state, a time window cast `as timestamp`, a list-parameter `count == 0` escape),
  are served by `admit` and `run` and answer the interpreter's rows; none answers `501`.
- A form outside this list stays refused whole, naming the filter.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong). ESS 0.99.1 is `d7833ee`, checked out at
the ESS 0.99.1 source (`d7833ee`) (Cargo.lock); below, `prim/` = `crates/specify/ess-primitives/src/`, `dom/` = `crates/specify/ess-domain/src/`.
Forms: F1 parameter vs literal, F2 text parameter under fold/trimmed, F3 `as timestamp`, F4 list-parameter `.count`, F5 member of a list-field element.

- **Primary surface:** `crates/spindle-host` (`src/view.rs`, filter admission only; evaluation is already ESS's own `Predicate::evaluate`, view.rs:918) — cited
- **Files:** `crates/spindle-host/src/view.rs:171-383` (`Scope::typed`, `Scope::admits`), `view.rs:80-88` (refusal text), `view.rs:29-68` (doc list) — cited
- **Symbols:** `Scope::typed` (no arm for `param.<x>.count`: the `[namespace, _]` arm at view.rs:201 takes two segments, and the three-segment arm at 220-223 leaves out `param`; no arm for `<binder>.<member>`: 209 and 222 leave out binders, so the path falls to `_ => None` at 227), `Scope::admits` (`typed.row` required for parameter-vs-literal at view.rs:311; `Defined` refuses binders at 320; no `FoldMatch`/`Trimmed` arm, both fall to `_ => false` at 381), `unsupported` — cited
- **Also likely:** new `crates/spindle-host/tests/spindle-host/<name>.rs` registered in `tests/spindle-host/main.rs`, plus a fixture dir under `tests/fixtures/`, using the `ess_conformance::interpret::Interpreted` comparison of `text_filters.rs:18` — inferred; `crates/spindle-host/src/host.rs:1625-1645` (second copy of the leaf list) — cited
- **Documents:** `CHANGELOG.md`; `ess/fixtures/observe/served/notes.json:51` quotes the refusal text as an example (Taskfile only schema-validates it, Taskfile.yml:267-273) — cited
- **Not touched:** `Read::absent_text_operands` (view.rs:644-662). ESS counts only `TextMatch` as reading a text parameter (prim/predicate.rs:2730-2744), so an absent parameter under a fold stays Unknown, and the interpreter refuses that read (`crates/verify/ess-conformance/src/interpret/views.rs:210-214`), as Spindle already does — cited. `parameters()` already binds a list parameter's `count` and elements through `ess_conformance::input::bind` (view.rs:495) — cited. The two non-filter refusals in the gap file (caller attributes; query-string inputs beside a body) are other surfaces — cited
- **ESS evaluation:** F1 `evaluate_compare` gives Unknown on an unobserved side and equality by value (prim/predicate.rs:2241-2244, 2338-2344). F2 `FoldMatch` (2207-2216) uses ASCII-only `FoldOp::holds` (536-540). `Trimmed` (2219-2222) trims only the wrapped operator's own subject (`trimmed_subject` 3053-3060, `TrimmedFacts` 568-630, whitespace set 559-564), and it also trims through a quantifier binder because `Element::rebind` maps `p` to `<list>.N` (1885-1911). F3 is no cast: the resolver tags `CompareKind::Instant` only when both facts are `Timestamp` (dom/expression.rs:1826-1855) and the checker refuses anything else (3092-3128); evaluation is at prim/predicate.rs:2250-2269. F4 resolves `<list>.count` as Integer (dom/expression.rs:1143-1150), and `observe` reads the bound `count` (prim/facts.rs:1564-1577). F5 is `Quantified::evaluate` over `<list>.N` (prim/predicate.rs:1716-1745) — cited
- **Safety fact:** F3 is already admitted today: the Fact/Fact `Compare` arm matches `..`, so it ignores `kind`, and two `Timestamp` facts are both ordered Text scalars (view.rs:284-298, `ordered` 409-420). F3 then needs a regression test only. Level 2, unproven — inferred
- **Confidence:** high for which leaf refuses each filter (each one traced to a view.rs line); medium for F3 already being read and for `call.*`/`integration.*` paths being read today (reference vs struct field not checked)
- **Would collide with:** any unit editing `crates/spindle-host/src/view.rs` `Scope::typed`/`Scope::admits` or the `unsupported` message, or the leaf-list doc in `host.rs:1625-1645`
- **Split:** do not split for parallel work. The parameter half (F1, F2, F4) and the list-element half (F5) both add arms to the same `typed()` path match (view.rs:185-228) and to the same `admits` match, and both rewrite the one message. Parallel halves would conflict at merge, so a split buys review size only and has to run in series — inferred

Refused leaf per filter (gap-file section ordinal; leaves generalised):
- §1: F2, `param.a in_ignore_case [...]` on a scalar parameter (view.rs:381) — cited
- §2: F1, `param.a == <enum word>` (view.rs:311) — cited
- §3: F1, `param.a == false` (view.rs:311) — cited
- §4: F1 `param.a == ""`; F5 `r.m == g` with `r` over a list field and `g` a list-parameter binder (view.rs:227); F2 `x equals_ignore_case "…"` on a list-parameter binder (view.rs:381) — cited
- §5: F5, `g.m == param.a` (view.rs:227) — cited
- §6: F2, `m equals_ignore_case "…"` on a list-parameter binder — cited
- §7: F1, `param.a == ""`; its `contains {param: a}` is already read — cited
- §8 and §9 (identical): F1 `param.a == false`; F2 `trimmed(param.a in_ignore_case [...])` and `trimmed(p equals_ignore_case "…")`; F5 `c.m == p` (outer binder); MISSING `defined(c.m)` (view.rs:227/320); MISSING `<row field> equals_ignore_case "…"` (view.rs:381). Its `as timestamp` leaves are admitted — cited (the `as timestamp` part inferred)
- §10: F1, `param.a == false` — cited
- §11: F2, `l in_ignore_case [...]` and `l equals_ignore_case "…"` on a list-parameter binder — cited
- §12: F1 `param.a == ""`; F2 `param.a equals_ignore_case "…"` on a scalar parameter — cited
- §13: F2, `s in_ignore_case [...]` on a list-parameter binder; `as timestamp` admitted — cited
- §14: F2, `trimmed(p equals_ignore_case "…")`; `as timestamp` admitted — cited
- §16-§19 (identical except one `type == <literal>` leaf): F4 `param.a.count == 0` (view.rs:201, 220-223); F1 `param.a == true/false`; `as timestamp` admitted — cited

Missing forms:
- A field of the row under a case fold, `<field> equals_ignore_case "…"` (§8/§9; ESS prim/predicate.rs:2207-2216). Adding it serves the view each of these fixtures keeps as its "still unread" example, so they need a new unread leaf: `tests/fixtures/views/board.yaml:118` (asserted by `tests/spindle-host/views.rs:195-204`), `tests/fixtures/always-and-key/domains/links.yaml:144` (`always_and_key_filters.rs:42,354-362`), spindle-cli `view-field-filters/domains/tray.yaml:165` (`view_field_filters.rs:49,462`), `adversary_unserved/domains/pad.yaml:53` (`adversary_unserved.rs:5`), spindle-ingress `served-operations/domains/pad.yaml:91` — cited
- `defined(<binder>.<member>)` for an element of a list field (`defined(c.m)`, §8/§9). F5 names only a comparison, and today `Defined` refuses binders (view.rs:320) — cited
- F5 compared with an outer list-parameter binder (`r.m == g`, `c.m == p`, §4/§8/§9), not only with `param.x`. The Outcome wording may cover it, but no acceptance test names it — inferred
- F3 is no missing form; it is a form already read (Safety fact), so its "one test per form" is a regression test — inferred

## Decided when scoped

- `param.<x> as timestamp` (F3) is admitted today; its test is a regression test.
- Three more forms the consumer's filters declare join the acceptance, one test each against
  `ess_conformance::interpret`: a row field under a case fold (`<field> equals_ignore_case "…"`),
  `defined(<binder>.<member>)` for an element of a list field, and a member of a list-field element
  compared with an outer list-parameter binder (`r.m == g`).
- Fixtures that keep a case-folded row field as their "still unread" example get another unread
  leaf: `tests/fixtures/views/board.yaml:118`, `always-and-key/domains/links.yaml:144`,
  spindle-cli `view-field-filters/domains/tray.yaml:165`, `adversary_unserved/domains/pad.yaml:53`,
  spindle-ingress `served-operations/domains/pad.yaml:91`.
- One unit, not split: both halves edit the same `typed()` and `admits` matches.

## Shared files

- `crates/spindle-host/src/view.rs`: this story's alone in wave 26.
- `crates/spindle-host/src/host.rs:1625-1645` (the leaf-list doc): shared with U3
  (`story:clock-and-mint-in-every-mode` edits `Opening`, `:984` and `reset_to` `:1195-1236`).
  Separate hunks, merged by the coordinator.
- `website/docs/concepts/specification.md`: this story extends the view-filter leaf list
  (`:139-155`); U1 (`story:related-selector-rows-quantifier`) edits `:65-68`. Separate hunks,
  merged by the coordinator.
- `crates/spindle-docs/src/status.rs` and its generated `website/docs/status.md` and
  `website/data/status.json`: this story adds or updates the view-filter row; U1, U3 and U4 edit
  other rows. Separate rows; the coordinator regenerates the pages after the merges.
- `crates/spindle-host/tests/spindle-host/main.rs`: this story adds a `mod` line; U3 adds one only
  if it tests the reset in that crate. Merged by the coordinator.
- Fixtures under spindle-cli and spindle-ingress named in "Decided when scoped": this story's alone.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.
