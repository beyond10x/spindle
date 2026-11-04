---
format: aep.planning-md/3
id: review-result:plan-acceptance-round-2
kind: review-result
status: active
title: Plan critic (acceptance), round 2
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
story:telemetry-transport — `a_qemu_instance_reports_over_vsock` and `a_firecracker_instance_reports_over_vsock` state only when the test is skipped ("skipped with its reason without `/dev/kvm` …") and never what is observed when it runs, so a test that is always skipped satisfies them. State the observation, as the process case does ("a collector on that socket receives frames with increasing sequence numbers") — .engineering/planning/story/telemetry-transport.md:58

What I read: 11 stories, all the ids you gave, through `aep plan artifact show` for each acceptance and header. I did not re-run `aep plan artifact kinds` or `lifecycle story`, which round 1 already covered. In the tree I confirmed `ui/src/lib/board.test.ts` and its two named tests, `ui/dev/views.json`, `ui/src/screens/InstanceScreen.vue` and `ViewScreen.vue` with no test, and `microvm.toml` binding `ingress.network` to `http`.

Round-1 findings, all five fixed:
- **admission `refused_names_every_missing_capability`:** now two checks.
- **placement `system_declares_spindle_observe`:** moved to "Already holds".
- **control-process retention:** `samples_older_than_fifteen_minutes_are_dropped` added.
- **ingress `the_listener_binds_loopback_only`:** split, with `an_ingress_binding_other_than_http_loopback_is_refused` added.
- **observe-ui screens:** the board is now named, and the instance and view screen checks are carried to `story:control-process-serves-the-api`, with the `observe-ui` Gap section recording it.

The telemetry-transport finding is new. It also held in round 1; I missed it then.

What I could not establish:
- I did not look at the `observe-ui` status. Its outcome still names the instance and view screens, which it now only carries elsewhere. The story leaves that to the operator, and I treated it as not a finding.
- Out of my lane, and it did not affect my verdict: the dependency edges added this round, including `story:run-over-http-ingress` depending on `story:replay-ess-scenarios`, and the telemetry wiring moved into `story:rally-runs-as-three-processes`.

```findings
- file: .engineering/planning/story/telemetry-transport.md
  line: 58
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: undecided
  message: "a_qemu_instance_reports_over_vsock and a_firecracker_instance_reports_over_vsock state only the skip condition and never what is observed when the test runs, so an always-skipped test satisfies them; state the observation (a collector on the vsock endpoint receives frames with increasing sequence numbers)"
```
