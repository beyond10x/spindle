---
format: aep.planning-md/3
id: review-result:wave32-sco-r1
kind: review-result
status: active
title: Plan critic (scope), round 1, on wave 32
relations:
- reviews: story:postgres-lane-runs-locally
- reviews: story:conform-awaits-system-steps
- reviews: story:leader-exits-promptly-on-sigterm
- reviews: story:seed-under-a-lease
revision: 1
---
needs-revision

Three findings, all about drafted promises that were narrowed or dropped without being recorded. The other two stories and the wave page keep what their drafts asked.

- `story:conform-awaits-system-steps` — the draft promises await scenarios for "a deadline, a tick, an event binding's command". The Outcome narrows this to commands granted only to `system: true` actors, and gives a reason: ESS 0.99.2 refuses such a binding, and deadlines and ticks are periodic bindings. D3 A also leaves a model with a periodic binding answering `unsupported`, so those scenarios stay skipped. `## Not covered` names only browser and Node hosts and does not record the deadline, tick and binding cases. Add them there, so the plan says less than the draft in plain sight. — `.engineering/planning/story/conform-awaits-system-steps.md:28` (Outcome), `:129-131` (Not covered); draft `546cc9d2:.engineering/planning/story/conform-awaits-system-steps.md:13-16`
- `story:conform-awaits-system-steps` — the draft promises "The skip count of the repository's own conformance suite for await scenarios is 0, and `check:conformance` names the count." D5 A replaces this with a count on a fixture, keeps the counts format unchanged, and rejects D5 C (the await skip count printed by `check:conformance`). The repository's own suite and the printed count are no longer claimed by any acceptance line, and nothing says the drop was decided. Either keep the printed await count or record the drop under `## Not covered`. — `.engineering/planning/story/conform-awaits-system-steps.md:60` (acceptance), `:101-107` (D5); draft `546cc9d2:.engineering/planning/story/conform-awaits-system-steps.md:30-31`
- `story:seed-under-a-lease` — the draft Outcome says the run "submits its seed when the store holds no record of a completed seed of that file". D5 B makes a store with rows and no record skip the seed and write a complete record. That contradicts the retained first sentence and narrows the draft, and nothing marks it as a deviation. State the exception where the first sentence promises the seed, or restate that sentence to match D5 B. — `.engineering/planning/story/seed-under-a-lease.md:41-43` against `:49-51` and `:83-85`; draft `546cc9d2:.engineering/planning/story/seed-under-a-lease.md:13-14`

Checked and not findings:
- **`story:postgres-lane-runs-locally`:**
  - The move from PostgreSQL 17 to "13 or later" with a warning off 17 is recorded as D3 A and in the wave page (`.engineering/waves/wave-32.md:23-24`).
  - The CA export, the no-cluster-when-a-URL-is-set case and the cleanup-after-SIGINT assertion follow from the Outcome's "TLS", "when no database is given" and "removes it afterwards".
- **`story:leader-exits-promptly-on-sigterm`:**
  - The bound named in `run --help`, 20 separate runs recorded as evidence, and "ready 200 within 10 s" all trace to the draft's "bound the docs name", "20 of 20" and "leads within one poll".
  - The 1 s per-statement release is the means to a bound that does not grow with `database_lease`, so it is not unasked work.
- **`story:seed-under-a-lease`:** the takeover, resume-after-count and upgrade tests, and the reset writes (D6), trace to "as `seed.json` keeps it" and to dropping D5 A of `story:several-writers-one-postgres`.
- **`story:conform-awaits-system-steps` (ESS pin):** keeping the pin at 0.99.2 is backed by the cited byte-identical `target.rs` across 0.99.2 to 0.99.10. It satisfies the draft's "if 0.99.2's differs".

What I read: 4 stories and the wave page, each story against its draft at `546cc9d2` (`git show`), plus `Taskfile.yml` and `conformance.toml` for the conformance count. Promises extracted from the drafts: 12 (postgres 3, conform 4, sigterm 2, seed 3). Traced to a claim: 9 fully, 3 narrowed as in the findings above. Reach beyond the drafts: none found.

What I could not establish:
- I did not run `aep plan artifact graph`. I compared the four stories against their drafts and the wave page only.
- I did not check whether the consumer's 50 await skips are `system: true` commands. That is out of my lane and bears on whether the narrowed conform Outcome serves its Why.
- Whether the 20 separate runs in `story:leader-exits-promptly-on-sigterm` count as a checkable acceptance is for the acceptance critic.

```findings
- file: .engineering/planning/story/conform-awaits-system-steps.md
  line: 129
  category: scope
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "the draft promises await scenarios for a deadline, a tick and an event binding's command; the Outcome narrows this to system:true actor commands (and D3 A leaves periodic-binding models unsupported) while Not covered names only browser and Node hosts, so the dropped cases are not recorded"
- file: .engineering/planning/story/conform-awaits-system-steps.md
  line: 60
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the draft promises 'The skip count of the repository's own conformance suite for await scenarios is 0, and check:conformance names the count'; the acceptance counts a fixture only and D5 C (printed await count) is rejected, so the repository-suite count and the printed count are claimed by nothing and the drop is not recorded"
- file: .engineering/planning/story/seed-under-a-lease.md
  line: 41
  category: scope
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the retained draft sentence says the run submits its seed when the store holds no record of a completed seed, but D5 B (lines 49-51, 83-85) skips the seed on a store with rows and no record, contradicting the first sentence and narrowing the draft without marking the exception"
```
