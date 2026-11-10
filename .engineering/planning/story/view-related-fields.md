---
format: aep.planning-md/3
id: story:view-related-fields
kind: story
status: active
title: A view field read through a reference is served
tags:
- downstream-gap
relations:
- serves: vision:spindle
- decomposes: epic:downstream-kernel-constructs
scope:
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: crates/spindle-cli/tests/spindle-cli/main.rs
- confidence: inferred
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-host/src/host.rs
- confidence: cited
  path: crates/spindle-host/src/view.rs
- confidence: inferred
  path: crates/spindle-host/tests/spindle-host/main.rs
- confidence: cited
  path: website/docs/concepts/specification.md
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-09T22:14:36Z", actor: "human:timo", revision: 2}
- {from: "proposed", to: "active", at: "2026-11-09T22:14:37Z", actor: "human:timo", revision: 3}
---
# A view field read through a reference is served

## Outcome

A view field declared `field: {related: {via: <reference field>, field: <field>}}` is read from the
referenced row. A downstream specification has 26 views with such a field (its count); today they
answer 501 (its measurement).

## Acceptance

On a neutral fixture with a view over `Grant` carrying `resource_name` read through `resource_id`
from `Resource.name`, each row carries the referenced resource's name, the field is absent when the
reference is empty or the row is gone as ESS 0.90.0's interpreter answers, and the answers equal
the interpreter's.

## Scope

Derived 2026-11-10 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `crates/spindle-host` (view reads) — cited: the 501 comes from `view::unsupported` returning "fields read through a reference" (`view.rs:53`), reached through `readable` (`host.rs:1731`), which `spindle_host::reads` (ingress `surface.rs:962`), `Host::list_with` (ingress `answer.rs:171`) and the conformance target's `host.list` (`target.rs:356`) go through
- **Files:** `crates/spindle-host/src/view.rs:48-54` (`unsupported`) and `:517` (`project`, reads only the source row today) — cited
- **Files:** `crates/spindle-host/src/host.rs:1222` (`Host::view`) and `:1293` (`Host::list_with`): each must load the row each `ResolvedViewReference.via` names, possibly from another component's store — inferred, `project` gets no store and `list_with` holds `&mut` on one store while it scans
- **Files:** `crates/spindle-host/src/host.rs:1286-1288` (doc comment) — cited
- **Symbols:** `ResolvedView.references`, `ResolvedViewReference { via, via_type, entity, fields }`, `ResolvedViewRead { field, read }` (ess-compiler `ir.rs:2403-2446`), `view::unsupported`, `view::project`, `readable`, `store_of` — cited
- **Reference behaviour:** ESS interpreter `interpret/views.rs:228-237` and `:246` (`referenced_rows`): a missing or `null` reference, or a gone row, puts `Node::Null` in the field; `read` may name a declared field, the identity or `state` — cited
- **Also likely:** new tests and a Grant/Resource fixture under `crates/spindle-host/tests/` and/or `crates/spindle-cli/tests/`, with `mod` lines in each `main.rs` — inferred, from commits 2a5bf3a5 and 7e695cf2
- **Documents:** `website/docs/concepts/specification.md:66-67` — cited; `CHANGELOG.md` `[Unreleased]`, `crates/spindle-docs/src/status.rs` near :214 — inferred
- **Confidence:** medium — the gate and the projection are cited; reading across component stores is inferred
- **Would collide with:** units touching `view.rs` `unsupported`/`project`, `host.rs` `view`/`list_with`/`readable`, the Views section of `specification.md`, `CHANGELOG.md`, and the test `main.rs` module lists of `spindle-host` and `spindle-cli`
- **Safety fact:** `ResolvedView.references` is read in one place (`view.rs:53`), and no test or `conformance.toml` entry pins a 501 for a reference view, so removing the gate changes no other caller's answer — unproven
- **Not established:** null versus omitted: the interpreter writes `Node::Null` before presence is applied (`views.rs:236`, `:479`), while Spindle's `project` drops nulls first; whether a filter through a reference stays refused; whether the referenced entity may belong to another component.
