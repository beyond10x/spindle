---
format: aep.planning-md/3
id: story:scenario-without-components
kind: story
status: draft
title: run --scenario replays a specification that declares no component
relations:
- serves: vision:spindle
- depends_on: story:scenario-act-without-outcome
revision: 6
---
# run --scenario replays a specification that declares no component

## Outcome

`run --scenario` replays a specification that declares no component, the way stdin mode runs one:
every command is accepted by one implied component that owns every domain. Today every act of such
a specification diverges with `no component accepts <command>` and the run exits 1, so a consumer
adds a generated component file to a copy of each specification before replaying it.

## Acceptance

- `a_scenario_replays_a_specification_without_components`: a specification with one command and no
  component, and a scenario with one act of that command, prints `holds` and exits 0.
- `a_scenario_on_declared_components_still_routes_by_accepts`: a specification whose components do
  not accept the act's command still diverges with `no component accepts <command>` and exits 1.
- `stdin_and_scenario_agree_on_a_specification_without_components`: the same command sent on stdin
  and as a scenario act answers the same outcome.

## Not covered

- Acts sent as a system actor: ESS's scenario authoring refuses them (`ESS-AUTHOR-044`,
  `ESS-AUTHOR-045`); Spindle applies ESS's rules unchanged. story:stdin-arrange-and-line-references
  covers sending such commands on stdin.

## Origin

A consumer's simulation, 2026-11-26, on 0.21.0, with a standalone reproduction.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

How stdin mode runs a specification without components — cited:
- `execute` opens no `Host` and has no component, implied or declared. It derives the whole
  system's requirements with no component (`crates/spindle-cli/src/main.rs:515`,
  `spindle_capability::derive(&loaded.ir, None)`) and admits them (`main.rs:521`). It opens one
  store for everything (`main.rs:656-689`). It runs each line with `spindle_kernel::execute_as`
  on that one store (`main.rs:705-711`, `:758-765`). Nothing routes by `accepts:`, so "one implied
  component that owns every domain" in the story describes the effect, not a mechanism in the code.

Why `--scenario` diverges today — cited, walked step by step (level 3):
- `replay` admits with `spindle_host::admit` (`crates/spindle-cli/src/replay.rs:77`). That function
  loops over `ir.components()` only (`crates/spindle-host/src/admission.rs:60`). With no component
  it returns `Ok` with no placements, so admission never refuses and the host opens empty
  (`crates/spindle-conformance/src/target.rs:121-130`).
- Each act calls `Host::submit_as` (`target.rs:246-248`), then `accepting`
  (`crates/spindle-host/src/host.rs:2169-2179`, called at `:1419`), which finds no component and
  returns `HostError::NotAccepted` (`host.rs:205-206`).
- `HostTarget` turns that into `TargetError::unavailable` (`target.rs:343-346`). The act prints
  `diverges` and the run exits 1 (`replay.rs:133-153`).

What `--scenario` needs — inferred, design choice A recommended:
- A. When `loaded.ir.components().is_empty()`, compile the specification again in memory with one
  generated component document that owns every domain (`ir.domains()`), accepts every command
  (`ir.commands()`) and publishes every event (`ir.events()`). This is the repro's workaround
  file, generated instead of written by hand. Compile through `spindle_spec::compile`
  (`crates/spindle-spec/src/lib.rs:147`).
  - The IR cannot be patched after it is compiled. `EssIr` keeps its components in a private
    field with only a getter (ess-compiler `ir.rs:3375`, `:3601`) and is `Serialize` only
    (`ir.rs:3325`).
  - `Specification` keeps file names, not file contents (`lib.rs:24-35`; the contents are dropped
    at `lib.rs:136`). So `spindle-spec` needs a load that keeps the texts or adds the implied
    component.
  - A specification that declares components keeps its IR unchanged, which keeps acceptance 2.
- B. Teach `spindle-host` an implied component (`admit`, `accepting`, `owner_of` at
  `host.rs:2214`, establish at `:1030`, `:1351`, view routing at `:2181`). This is wider: it
  changes `play`, `serve`, `deploy`, `conform` and embed alike. Not recommended.

- **Primary surface:** `crates/spindle-cli/src/replay.rs` (`replay`, `:44-89`) — cited
- **Files:** `crates/spindle-cli/src/replay.rs:44-89` — cited; `crates/spindle-spec/src/lib.rs:83-144`
  (a load that keeps the source texts or adds the implied component) — inferred
- **Files:** `crates/spindle-cli/tests/spindle-cli/replay.rs` and a new spec without components
  under `crates/spindle-cli/tests/fixtures/` — inferred (the existing replay tests and fixtures,
  `fixtures/replay`, `fixtures/replay-scenarios`)
- **Symbols:** `replay::replay`, `spindle_spec::load`, `spindle_spec::compile`,
  `EssIr::components` — cited
- **Also likely:** `crates/spindle-cli/src/cli.rs:93-99` (the `--scenario` help says "every
  component in one process") — inferred
- **Also likely:** the stdin side of acceptance 3 lives in an existing `spindle-cli` run test —
  inferred, file not located
- **Documents:** `crates/spindle-docs/src/status.rs:450-455` ("Scenario replay") — inferred
- **Not touched (choice A):** `crates/spindle-host`, `crates/spindle-conformance/src/target.rs`,
  `crates/spindle-cli/src/main.rs`, and `ess-*` pinned at 0.99.1 (`Cargo.toml:17-23`) — inferred
- **Not touched:** `crates/spindle-cli/src/conform.rs:122` has the same gap (`HostTarget` on an IR
  without components). The story does not cover it; a load-time helper in `spindle-spec` could
  serve both — inferred
- **Confidence:** high for the cause and for `replay.rs`; the code path is read end to end. Medium
  for the files, because choice A versus B decides whether `spindle-spec` or `spindle-host`
  changes.
- **Would collide with:** any unit touching `spindle-cli`'s `replay.rs` or its replay tests and
  fixtures (cited: the earlier unit in this wave edits both). Under A, any unit touching
  `spindle-spec`'s loader (`load`, `Specification`). Under B, every unit touching
  `spindle-host`'s routing or admission — inferred
- **Safety fact:** the implied component is added only when `ir.components()` is empty, so a
  specification that declares components replays on the identical IR and still routes by
  `accepts:` (`host.rs:2169-2179`). Level 2, unproven.
- **Safety fact (behaviour change):** with an implied component, `spindle_host::admit` derives
  real requirements for it, so a specification without components can now be refused (exit 3) on
  an environment that lacks a capability. Today it is never refused, because the loop at
  `admission.rs:60` is empty. Stdin mode already refuses in that case (`main.rs:521-525`), so this
  aligns the two modes. Level 2, unproven.

Gap confirmed: yes, by reading (level 3, nothing run). The repro's `ess/` declares no component,
and its `replay-component.yaml` (owns `repro.core` and `repro.actors`, accepts both commands,
publishes both events) is the manual workaround that choice A would generate.

## Decided when scoped

- Choice A: when the compiled specification declares no component, `--scenario` compiles it again in
  memory with one generated component that owns every domain, accepts every command and publishes
  every event. `spindle-spec` gains a load that keeps the source texts or adds that component;
  `spindle-host` does not change. A specification that declares components replays on its
  unchanged IR.
- Admission then derives the implied component's requirements, so a specification without
  components can be refused (exit 3) on an environment that lacks a capability, as stdin mode
  already refuses it. A test names this.
- If the generated component does not compile under the ESS 0.99.1 pin, the unit reports the
  compiler's refusal and stops.
- Not covered: `conform` on a specification without components (`conform.rs:122`), the same gap; a
  draft follows if the helper does not cover it for free.

## Shared files

- `crates/spindle-cli/src/replay.rs` (`replay`, `:44-89`, the admission at `:77-82`) and its tests:
  shared with `story:scenario-act-without-outcome` in the same unit (U4), built after it on one
  branch (`depends_on`), and with U3, which edits `Replays::run` at `:187`; separate hunks, merged by
  the coordinator after U3.
- `crates/spindle-spec/src/lib.rs`: this story's alone in wave 26.
- The `--scenario` help in `cli.rs`, the generated `cli.md` and `status.rs:450-455`: shared with U3;
  separate hunks.
- `CHANGELOG.md` `[Unreleased]`: one bullet per story, merged by the coordinator.
