---
format: aep.planning-md/3
id: review-result:wave28-acc-r1
kind: review-result
status: active
title: Plan critic (acceptance), round 1, on wave 28
relations:
- reviews: story:console-routes-off
- reviews: story:structured-json-logs
- reviews: story:metrics-series-bounded
- reviews: story:seed-readiness
- reviews: story:health-and-graceful-drain
- reviews: story:entity-event-history
- reviews: story:events-record-their-caller
- reviews: story:environment-binds-external-decisions
- reviews: story:replay-with-decisions
- reviews: story:one-time-response-admission
revision: 1
---
needs-revision

- story:one-time-response-admission — acceptance bullets 2 and 3 do not say they fail on 0.22.0, so they may pass before the work. The scope states the defect is only in the replay prefix, and that a one-time model already runs under `conform` (`adversary_responses.rs:25`). Only bullet 1 carries "It fails on 0.22.0". Name which of bullets 2 and 3 fail on 0.22.0, or drop the ones that cannot. The Outcome also still says "refused at admission … from target.rs", which the Scope calls stale. — `.engineering/waves/wave-28.md` is not the cite; the story body, Acceptance bullets 2-3 (the story's own scope lines "Stale in the body")
- story:seed-readiness — no acceptance observes the thing the readiness signal is for: after `/__spindle/ready` reads 200, the views hold the seed's last entry. The Scope's own safety fact says this is "not proven". The acceptance only checks the state name and counts. Add a test that reads a view row right after the first 200. — Acceptance, and Scope "Safety fact" on the `ended` flip after `Host::submit_as`
- story:metrics-series-bounded — "writes at most the number of series the test states" leaves N to the test author, and one run over 300 commands cannot show it "does not grow with the number of commands". Fix N in the story, or require the same series count at two specification sizes. — Acceptance bullet 2 (`an_aggregated_scrape_of_300_commands_writes_at_most_n_series`)
- story:entity-event-history — the acceptance does not say what a read of an instance answers on a store written before this change, where the new per-write instances field is absent. That is indistinguishable from the "reads empty" case in bullet 3. Add an old-store case, either "reads empty" or "reads from the origin". — Acceptance, and Scope "Safety fact" ("read back as absent on old data")
- story:structured-json-logs — decision D2 (`--log-format json` on a run that does not serve HTTP exits 1 before reading stdin) has no acceptance bullet. The Outcome field `request_id` also appears in no acceptance, and D4 says there is no per-request line. Add a test for the D2 refusal, and say which line carries `request_id`. — Acceptance (3 bullets), Decided D2 and D4
- story:events-record-their-caller — decision D2 (in a split run the event names the binding and records the cause as not known) has no acceptance bullet. Bullet 2 covers only the in-process case. Add a split-run test. — Acceptance bullet 2, Decided D2
- story:environment-binds-external-decisions — decision D3 (`decisions-file` bound with no file named exits 1 before running) has no acceptance bullet. Add one. — Acceptance (5 bullets), Decided D3

What I read: all 10 stories in full with `aep plan artifact show story:<id>`, after `aep plan artifact kinds` and `aep plan artifact lifecycle story`. I ran `git grep` on `adversary_responses.rs` and sampled lines 1-45. I grepped the bodies for personal names, hosts, URLs and home paths and found none.

What I could not establish:
- Whether the PostgreSQL and IndexedDB contract cases in `story:events-record-their-caller` and `story:entity-event-history` run in the gate. Scope says they are ignored without `SPINDLE_TEST_DATABASE_URL`.
- Whether the new bounds and timings hold: no test was measured, and the stories say so.
- Out of my lane, not counted in the verdict:
  - `story:health-and-graceful-drain` repeats the `/__spindle/ready` route and its trusted-caller 403 test from `story:seed-readiness`.
  - `story:one-time-response-admission` and `story:replay-with-decisions` share `replay.rs`.

```findings
- file: story:one-time-response-admission (Acceptance bullets 2-3; Scope "Stale in the body")
  category: acceptance
  severity: blocker
  verdict: needs-revision
  origin: introduced
  message: "acceptance bullets 2 and 3 do not say they fail on 0.22.0 and the scope says a one-time model already runs under conform, so they may pass before the work and show no transition; only bullet 1 says it fails on 0.22.0"
- file: story:seed-readiness (Acceptance; Scope Safety fact on the ended flip)
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "no acceptance reads a view row after the first 200 on /__spindle/ready, so the promise that a probe needs no view rows rests on a commit-visibility fact the scope calls unproven"
- file: story:metrics-series-bounded (Acceptance bullet 2)
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "writes at most the number of series the test states leaves N to the test author and one run over 300 commands cannot show the count does not grow; fix N or compare two specification sizes"
- file: story:entity-event-history (Acceptance; Scope Safety fact)
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "the acceptance does not say what a read of an instance answers on a store written before the per-write instances field existed, which is indistinguishable from the reads-empty case"
- file: story:structured-json-logs (Acceptance; Decided D2, D4)
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "decision D2 (json format on a non-serving run exits 1 before reading stdin) has no acceptance bullet, and request_id is named in the Outcome but in no acceptance while D4 removes the per-request line"
- file: story:events-record-their-caller (Acceptance bullet 2; Decided D2)
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "decision D2 (split run records the cause as not known) has no acceptance bullet; bullet 2 covers only the in-process case"
- file: story:environment-binds-external-decisions (Acceptance; Decided D3)
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "decision D3 (decisions-file bound with no file named exits 1 before running) has no acceptance bullet"
```
