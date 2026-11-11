---
format: aep.planning-md/3
id: review-result:w19-u2-adversary-2
kind: review-result
status: active
title: Wave 19 U2 adversary pass 2
relations:
- reviews: story:http-caller-proves-actor
revision: 1
---
approve

```
unit: wave 19 U2 story:http-caller-proves-actor, commits 79be6ef0 + 9d8aff05 (uncommitted test files only)
verdict: nothing found
cases: executed 403→409, red 0
origin: introduced 0 / pre-existing 0 / undecided 0
```

Cases added, all green:

- `crates/spindle-ingress/tests/adversary2_caller_proof.rs`: 20,000 generated proofs (fixed seed) with every key spelling (`\uXXXX` either case, `\/`, surrogate pairs) and nested values full of JSON punctuation, refused exactly when an object repeats a decoded name; every spelling of one name is that name, NFC and NFD stay two; depth 200,000 refused without overflow; 400 kB proofs answered in under 5 s.
- `crates/spindle-cli/tests/adversary2_console_paths.rs`: more than 3,000 raw requests without a valid secret against every console and reserved route in every path, method, version and header spelling tried, with reset worlds in every POST: no 2xx, no state in any answer, the store unchanged afterwards; a pipelined request carries no secret from the one before.

Suite: `cargo test -p spindle-cli -p spindle-ingress`: spindle-cli 350 passed, 3 failed (known ui/dist); spindle-ingress 13 + 34 + 4; the new console file 2.

Attacked and not broken: the secret is checked by route kind wherever a route resolves as reserved or console; nothing but the dev token and the path is read before it; `unique_members` agrees with serde_json on valid JSON, iterates without recursion, O(n log n); the six pass-1 cases still hold.

```findings
[]
```
