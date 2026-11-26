---
format: aep.planning-md/3
id: story:cross-component-reads-in-deploy-and-embed
kind: story
status: draft
title: Deploy and an embedded host serve a cross-component guard
relations:
- serves: vision:spindle
- informed_by: story:cross-component-related-reads
revision: 1
---
## Outcome

A command whose guard reads another component's rows is served by `deploy` and by an embedded host
where the host can read the owner's store, as `run` serves it since
`story:cross-component-related-reads`. `spindle-control` and `spindle-embed` still build their
surfaces with `Surface::new`, which reaches only the running component, so such a command is marked
501 there although the host reads the owner's store.

Found by the wave 27 U8 implementor.

## Acceptance

- `an_embedded_host_serves_a_cross_component_guard`: `spindle-embed` opened on the
  `metering-split` fixture with both components answers the guarded command as the interpreter
  does, not 501.
- `deploy` places such a command as served where its components share one process, and refuses it
  by name where they do not.
