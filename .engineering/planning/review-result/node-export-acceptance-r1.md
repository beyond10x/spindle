---
format: aep.planning-md/3
id: review-result:node-export-acceptance-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on the Node export stories
relations:
- reviews: story:node-export-serves-a-specification
- reviews: story:node-export-fixed-actor
- reviews: story:node-export-clock-and-seeds
- reviews: story:node-export-events
- reviews: story:node-export-release-artifact
revision: 1
---
needs-revision

story:node-export-serves-a-specification — item 5 bundles seven independent refusals under one test, names no fixture for any of them, and says only that each message "names the reason" without saying whose reason or how it is compared (items 1-3 compare against `run`), so one refusal can fail while the test is judged on another — .engineering/planning/story/node-export-serves-a-specification.md:80
story:node-export-serves-a-specification — item 4 does not say where the expected lines come from: "the lines `run` prints on stderr" is not tied to a recorded native output (items 1 and 2 name one), so `notServed()` could be checked against `Surface::report` itself and pass whatever it prints — .engineering/planning/story/node-export-serves-a-specification.md:75
story:node-export-events — the outcome promises "a reset ends what was delivered before it" and no acceptance bullet checks it, and the callback bullet joins two outcomes (once after the publishing request, and none after `dispose()`) — .engineering/planning/story/node-export-events.md:19
story:node-export-release-artifact — "loads in the Node lane's first test ... when pointed at the extracted archive" names no way to point the test at an archive, and neither this story nor `story:node-export-serves-a-specification` provides that switch — .engineering/planning/story/node-export-release-artifact.md:27

What I read: the five ids, with `aep plan artifact show <id>` for each, plus `aep plan artifact lifecycle story`. I checked the tree with `ls`, `git grep` and `sed` on `crates/spindle-cli/src/main.rs` and `serve.rs`.
- Confirmed present: the review fixture (`decisions.toml`, `exchanges.json`), the `outcome-scoped-refusal` fixture, the native test `the_loopback_listener_answers_the_recorded_exchanges`, and the `check:the_wasm_build_of_the_host_succeeds` lane.
- Confirmed absent, so the story creates it: `check:node_serves_a_specification`.

What I could not establish:
- Whether `Surface::report` and `spindle_host::read_world` can be extracted as the design says. That is the design critic's lane, so it does not set my verdict.
- Whether `outcome-scoped-refusal` yields both kinds of line (operations not served, outcomes answered `501`). I did not run `run` against it.
- `story:node-export-fixed-actor` and `story:node-export-clock-and-seeds` have checkable acceptances. I left the thinner points (the clock-start fixture is unnamed) out as draft thinness.
- Items 1-3 and 6-8 of the story to implement now, and its transition from "no Node export" to "one exists", are observable. I raised no finding on them.

```findings
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 80
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "item 5 bundles seven independent refusals under one test, names no fixture for any of them, and says only that each message 'names the reason' without saying whose reason or how it is compared, so one refusal can fail while the test is judged on another"
- file: .engineering/planning/story/node-export-serves-a-specification.md
  line: 75
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "item 4 does not say where the expected lines come from: 'the lines `run` prints on stderr' is not tied to a recorded native output, so notServed() could be checked against Surface::report itself and pass whatever it prints"
- file: .engineering/planning/story/node-export-events.md
  line: 19
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the outcome promises 'a reset ends what was delivered before it' and no acceptance bullet checks it, and the callback bullet joins two outcomes (once after the publishing request, and none after dispose())"
- file: .engineering/planning/story/node-export-release-artifact.md
  line: 27
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "'loads in the Node lane's first test when pointed at the extracted archive' names no way to point the test at an archive, and neither this story nor story:node-export-serves-a-specification provides that switch"
```
