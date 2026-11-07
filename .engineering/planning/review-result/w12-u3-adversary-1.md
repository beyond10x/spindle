---
format: aep.planning-md/3
id: review-result:w12-u3-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on http-outcome-bodies (wave 12)
relations:
- reviews: story:http-outcome-bodies
revision: 1
---
Adversary pass 1 on `story:http-outcome-bodies`, wave 12 unit u3, change 5315f8f; the
adversary's 6 green cases are committed as 53e45ed (module `adversary_outcome_bodies`, fixture
`crates/spindle-cli/tests/fixtures/adversary-outcome-bodies/`). Recorded from the adversary's
report; absolute scratch paths are left out (the repository is public).

```
verdict: NEEDS-CHANGE
cases: executed 222→231, red 3
origin: introduced 2 / pre-existing 0 / undecided 1
```

Red, uncommitted at report time:

```
a_response_body_keys_a_renamed_response_field_by_its_wire_name
  `body: response` ... `receipt_code` under its wire name `receiptCode`: {"receipt_code":"…","slot":…}
a_response_field_body_of_a_struct_keys_its_members_by_wire_name
  `response.ticket` ... its member `ticket_code` under its wire name `ticketCode`: {"row":…,"ticket_code":"…"}
the_envelope_response_keys_a_renamed_field_by_its_wire_name
  the envelope's `response` carries `receipt_code` under its wire name `receiptCode`
```

Green, committed: a renamed `response.<field>` scalar; an `Integer` bare body as
`application/json`; `body: none` on 200 (`content-length: 0`, no `content-type`); `body: none` on
two refusals grouped with `alike` (409, empty, nothing written); an outcome bound with a body the
kernel cannot run (501 naming it, nothing written, the other outcome still served); the
transport-free router answering `body: none` with `""`.

The full spindle-cli binary in the unit's tree also failed 5 `conform::*` cases
(`conformance-known-failures.json` bound to the specification digest before wave 12's observe
domain change) and `deploy::the_ui_is_served_from_the_binary` (`ui/dist` not built); neither is
this unit's.

```findings
[{"file": "crates/spindle-ingress/src/answer.rs", "line": 332, "category": "acceptance", "severity": "blocker", "verdict": "NEEDS-CHANGE", "origin": "introduced", "message": "body: response answers the kernel's local-name-keyed response, so a response field with a wire name is written under its local name and the body fails the schema ESS projects and differs from what ESS's served application answers."}, {"file": "crates/spindle-ingress/src/answer.rs", "line": 337, "category": "contract-drift", "severity": "blocker", "verdict": "NEEDS-CHANGE", "origin": "introduced", "message": "body: response.<field> whose field is a struct answers the struct's members under their local names rather than the wire names ESS publishes."}, {"file": "crates/spindle-ingress/src/answer.rs", "line": 314, "category": "contract-drift", "severity": "warning", "verdict": "CONFIRMED", "origin": "undecided", "message": "The envelope's response member keys wire-renamed response fields by local name, so the envelope fails the schema ESS projects for the outcome."}]
```

Coordinator's routing: findings 1 and 2 back to the implementor. Finding 3 is pre-existing (the
envelope path is unchanged by the unit), but its fix is the same wire-name mapping in the same
function, and a bare body keyed by wire name beside an envelope keyed by local name would be two
contracts for one response, so the unit takes it.
