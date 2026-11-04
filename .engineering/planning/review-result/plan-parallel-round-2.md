---
format: aep.planning-md/3
id: review-result:plan-parallel-round-2
kind: review-result
status: active
title: Plan critic (parallel), round 2
relations:
- reviews: story:spindle-admission-rules-are-declared
- reviews: story:spindle-placement-rules-are-declared
- reviews: story:rally-rules-are-declared
- reviews: story:telemetry-transport
- reviews: story:control-process-serves-the-api
- reviews: story:replay-ess-scenarios
- reviews: story:run-over-http-ingress
- reviews: story:rally-runs-as-three-processes
- reviews: story:rally-runs-in-three-microvms
- reviews: story:spindle-conforms-to-its-specification
- reviews: story:observe-ui
revision: 1
---
needs-revision
story:control-process-serves-the-api — both it and story:spindle-conforms-to-its-specification edit `Taskfile.yml` (cited, both bodies: this one changes `build` and `check` to build `ui/` first, the other adds a `check:conformance` step to `check`). No `depends_on` edge or body mention orders them, and `aep plan artifact waves` still prints `collision: story:control-process-serves-the-api story:spindle-conforms-to-its-specification Taskfile.yml`. The remedy is an ordering edge recording `Taskfile.yml` as its reason, or a separate task file for the conformance step — .engineering/planning/story/control-process-serves-the-api.md:Scope "`Taskfile.yml` — cited", .engineering/planning/story/spindle-conforms-to-its-specification.md:Scope "`Taskfile.yml` — cited"

**What changed since round 1.** Most round-1 findings are fixed.
- **telemetry-transport and replay-ess-scenarios:** telemetry-transport now names `crates/spindle-cli` and `crates/spindle-host` as not in scope. Its sender is a hook in `crates/spindle-telemetry` (`Counters::snapshot` at `src/counters.rs:150`), and its test drives the existing stand-in `crates/spindle-target/src/bin/spindle-target-fixture.rs`. The two stories now share no source file, so wave 3 can run them together. They still share `status.rs`, README.md, AGENTS.md and `Cargo.lock`. telemetry-transport names these and says the wave coordinator resolves them at integration. That is a named, deliberate collision, so it is not a finding.
- **spindle-cli cluster:** the edges run replay, then ingress, then three-processes (`run-over-http-ingress` depends on `replay-ess-scenarios`; `rally-runs-as-three-processes` depends on `run-over-http-ingress`).
- **Guest init:** `rally-runs-in-three-microvms` depends on telemetry-transport and the body names the init files.
- **ess/ specification stories:** conforms depends on admission and placement; placement and rally-rules depend on admission; the Order sections name the shared surfaces.

**What you read:** 10 artifacts via `aep plan artifact waves` and `graph`, the story files in `.engineering/planning/story/`, and the tree under `crates/` for the hook and binaries. Surfaces: 10 cited, 0 inferred, 0 unplaceable. The `waves` output marks several surfaces "(inferred)", but each body now names its surface. Acceptance reads traced to a producer in the set: 7, all 7 recorded by a `depends_on` edge.

**Could not establish:**
- Whether the telemetry hook in `crates/spindle-telemetry` needs a new dependency in `Cargo.toml`. If it does, it meets replay's `Cargo.toml` edit; `Cargo.lock` is already named shared.
- `aep plan artifact waves` still prints `collision:` lines for pairs the edges now order (for example `story:replay-ess-scenarios story:run-over-http-ingress crates/spindle-cli`). I read them as the tool not weighing the new edges, not as live collisions.
- Out of my lane: whether untyped surfaces such as `status.rs` should be typed scope.

```findings
- file: .engineering/planning/story/control-process-serves-the-api.md
  category: parallel-safety
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "edits Taskfile.yml build and check (Scope, `Taskfile.yml` cited), which story:spindle-conforms-to-its-specification also edits (check:conformance step in check, cited in both bodies); no depends_on edge or body mention orders them and the waves output still reports the collision"
```
