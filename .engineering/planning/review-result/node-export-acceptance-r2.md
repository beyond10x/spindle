---
format: aep.planning-md/3
id: review-result:node-export-acceptance-r2
kind: review-result
status: active
title: Plan critic (acceptance), round 2, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
needs-revision

story:node-export-serves-a-specification — item 5 says "the message compared in full" but three of its bullets say "starts with" (`the configuration: unknown field`, `the specification: `, `the environment: `), so the comparison the test makes is stated two ways — .engineering/planning/story/node-export-serves-a-specification.md:98
story:node-export-serves-a-specification — item 6 joins two independent outcomes ("two instances share nothing and a disposed one refuses") under one test, so isolation can fail while disposal passes — .engineering/planning/story/node-export-serves-a-specification.md:119
story:node-export-serves-a-specification — item 3 joins three independent outcomes (decisions veto `502`, answer default branches, refusal with neither) under one test, and the last repeats item 5's `refuses external outcomes nothing answers` — .engineering/planning/story/node-export-serves-a-specification.md:84
story:node-export-release-artifact — the outcome promises the archive holds the `.wasm`, the glue and the declarations and is covered by `SHA256SUMS`, and no acceptance bullet checks either (`sha256sum --check` passes, the archive lists those three files) — .engineering/planning/story/node-export-release-artifact.md:29
story:node-export-clock-and-seeds — the outcome promises that the host program sets the clock and that "any randomness the host uses is seeded too", and no bullet checks either: the bullets cover a start instant, `advance` and a mint seed — .engineering/planning/story/node-export-clock-and-seeds.md:23

What I read: 5 of 5 ids (`aep plan artifact show` on each), plus the epic and `review-result:node-export-acceptance-r1`. I checked the tree with `ls` and `git grep` for fixtures, tests and the release workflow.
- Round 1 findings that landed:
  - Item 5 now has one test per refusal, each with a fixture and an `(R)` marker for the reason taken from `run`.
  - Item 4 is tied to a recorded `not-served.json` and a native test that starts `run`.
  - The events story has a reset bullet, and its callback bullet was split into a once-per-event bullet and a disposal bullet.
  - `SPINDLE_NODE_MODULE` is now the switch for pointing the lane at an archive.
- Fixtures and native tests named by the acceptances exist: `review`, `outcome-scoped-refusal`, `unservable`, `outcome_scoped_refusal.rs` and `the_loopback_listener_answers_the_recorded_exchanges`.

What I could not establish:
- Whether the `outcome-scoped-refusal` fixture with its binding prints lines for both operations not served and outcomes answered `501`. I did not run `run` against it.
- Whether a mint seed changes the output of `story:node-export-clock-and-seeds` at all. Two unseeded instances may already answer identical bytes. A bullet showing that a different seed gives different identities would settle it. I did not raise it because I could not tell.
- Whether `Surface::report`, `spindle_host::read_world` and the `spindle-embed` split can be extracted as designed. That is the design critic's lane and does not set my verdict.
- `story:node-export-fixed-actor` and the first bullet of the events story are checkable. I left their unnamed fixtures out as draft thinness.

```findings
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 98
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "item 5 says the message is 'compared in full' but three of its bullets say 'starts with', so the comparison the test makes is stated two ways"
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 119
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "item 6 joins two independent outcomes ('two instances share nothing and a disposed one refuses') under one test, so isolation can fail while disposal passes"
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 84
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "item 3 joins three independent outcomes (decisions veto 502, answer default branches, refusal with neither) under one test, and the last repeats item 5's 'refuses external outcomes nothing answers'"
- file: .engineering/planning/story/node-export-release-artifact.md
  line: 29
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome promises the archive holds the .wasm, the glue and the declarations and is covered by SHA256SUMS, and no acceptance bullet checks either"
- file: .engineering/planning/story/node-export-clock-and-seeds.md
  line: 23
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome promises that the host program sets the clock and that any randomness the host uses is seeded, and no bullet checks either: the bullets cover a start instant, advance and a mint seed"
```
