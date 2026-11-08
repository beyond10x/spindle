---
format: aep.planning-md/3
id: review-result:w16-u1-adversary-1
kind: review-result
status: active
title: Adversary pass 1 on kernel-creation-takes-unknown-row (wave 16)
relations:
- reviews: story:kernel-creation-takes-unknown-row
revision: 1
---
needs-change

Adversary pass 1 on story:kernel-creation-takes-unknown-row (wave 16 U1), attacking 92a8388e.
Fourteen cases in `crates/spindle-kernel/tests/spindle-kernel/adversary_creation_unknown.rs`
(commit 0c2c2f31), each run on a fresh store on the kernel and on ESS's interpreter at 0.90.0 and
compared step by step; 5 red on 92a8388e.

- F1, blocker, introduced, fixed in the unit (3ba21f52): an absent addressed row on a command with
  no row set, whose selected branch does not create, reached present-related refusals (bay-shut,
  no-bay) before unknown_instance; the interpreter answers unknown_instance first.
- F2, warning, pre-existing, fixed in the unit (3ba21f52): the existence step's fallback answered
  wrong_state or Undeclared without consulting the command's declared not-found refusal (a
  command with no creation, a delete).
- F3, warning, introduced, fixed in the unit (3ba21f52): the declared not-found refusal minted a
  random value into an error field it gives no source; the interpreter leaves the field out.
  Ruling: leave it out, as the interpreter does.
- F4, note: `DeclaredError::unknown_instance` stays false for the declared not-found answer;
  status unchanged, payload kept. No change.

All 14 cases green on 3ba21f52; merged into wave 16 as 0dd405ed.

```findings
[
{"file":"crates/spindle-kernel/src/execute.rs","line":990,"category":"acceptance","severity":"blocker","verdict":"NEEDS-CHANGE","origin":"introduced","message":"an absent addressed row on a no-row-set command whose selected branch does not create reaches present-related refusals before unknown_instance"},
{"file":"crates/spindle-kernel/src/execute.rs","line":508,"category":"acceptance","severity":"warning","verdict":"CONFIRMED","origin":"pre-existing","message":"the existence step's fallback answers wrong_state or Undeclared without consulting the declared not-found refusal"},
{"file":"crates/spindle-kernel/src/execute.rs","line":1152,"category":"acceptance","severity":"warning","verdict":"CONFIRMED","origin":"introduced","message":"the declared not-found refusal mints a random value into an unsourced error field; the interpreter leaves it out"},
{"file":"crates/spindle-kernel/src/lib.rs","line":119,"category":"judgement","severity":"note","verdict":"CONFIRMED","origin":"introduced","message":"DeclaredError::unknown_instance stays false for the declared not-found answer; status and payload unchanged"}
]
```
