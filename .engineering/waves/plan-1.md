# Wave plan 1

Derived 2026-11-04 by `aep plan artifact waves --kind story --status active` from the stories the
operator accepted (`approval-record:wave-1-acceptance`). All serve `vision:spindle`.

| wave | stories | page |
|---|---|---|
| 1 | `story:execute-commands`, `story:periodic-bindings-fire`, `story:bindings-cross-components` | [wave-1.md](wave-1.md) |
| 2 | `story:rally-is-watchable-in-a-terminal` | not yet written |

Verb output, verbatim:

```
wave 1
  story:bindings-cross-components
  story:execute-commands
  story:periodic-bindings-fire
wave 2
  story:rally-is-watchable-in-a-terminal
collision: story:execute-commands story:rally-is-watchable-in-a-terminal crates/spindle-cli/src/main.rs
2 wave(s), 1 collision(s), 0 unassessed
```

Wave 2 also `depends_on` all three stories of wave 1.
