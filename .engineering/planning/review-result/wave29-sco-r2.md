---
format: aep.planning-md/3
id: review-result:wave29-sco-r2
kind: review-result
status: active
title: Plan critic (scope), round 2, on wave 29
relations:
- reviews: story:binding-failure-escalates
- reviews: story:payload-field-through-conversion
- reviews: story:value-source-through-two-rows
- reviews: story:ess-pin-0-99-2
revision: 1
---
approve

**What I read:** the 4 stories at head 4f901853, and the parent drafts they came from (`git show 058698a9:` for the escalation story, `git show c4cc6c3b:` for the other three). I also read `.engineering/waves/wave-29.md`, `review-result:wave29-sco-r1`, `git log` of each story and the message of e0798f58, and ran `aep plan artifact graph` and `aep plan artifact validate` (result: valid). I took 23 promises from the parent drafts and traced all 23 to an item.

Both round-1 findings are fixed:
- **Escalation story:** `story:binding-failure-escalates` Acceptance line 88 now requires every scenario that selects an escalated refusal to run, with none skipped as `unsupported`.
- **Conversion story:** `story:payload-field-through-conversion` Acceptance lines 55-57 now require the same for the outcome with the conversion.

**The full re-check:**
- **No silent narrowing.** Every place a story now promises less than its parent draft is recorded under "Decided when scoped":
  - escalation D2 A (the event must be in the component's `publishes:` list) and D4 A (`unmet` when the input lacks a field the event needs)
  - conversion D1 A (`{response: ...}` fields stay refused; values pass on unchanged)
  - ESS pin D1 A (exactly 0.99.2, not "or later") and D2 A (the tests of four crates instead of the whole workspace). This D2 A also settles the test-scope point I left open in round 1.
- **No reach beyond the parent.** The added escalation cases (universal policy, stdin line, `--drop-retries`, dispatch to further bindings) and the conversion check on chained reads all follow from the parent's "as ESS 0.99.1 defines/reads it".
- **No outcome claimed twice.** The conversion check on one-hop reads is in `payload-field-through-conversion` D1 A. The check on chained reads is in `value-source-through-two-rows` D4 A, which says it builds on the first. The second story `depends_on` the first.
- **Public repository.** No person, organisation, host or home path in the four bodies or the wave page. `b10x-spindle` is the binary's published name (it appears in `README.md` and `crates/spindle-cli/Cargo.toml`).

**What I could not establish:**
- **Escalation, D2 A:** the decision is recorded, but I could not tell whether the consumer's own specification declares the escalation event in `publishes:`. If it does not, that specification is still refused at start.
- **Conversion, D1 A:** the decision is recorded, but I could not tell whether the consumer's stamped instant comes from a `{response: ...}` field. If it does, the consumer's gap stays open.
- **Conform lines (escalation and conversion):** the words "on 0.22.0 each is" skipped assume the synthesised suite contains such scenarios. That is for the acceptance lane, not mine.

```findings
[]
```
