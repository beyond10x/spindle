---
format: aep.planning-md/3
id: review-result:wave31-adversary-U4-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U4
relations:
- reviews: story:verified-caller-token
revision: 1
---
unit: story:verified-caller-token, findings cover head e0e5b04d (the unit's 83f1df99 plus my test commit)
verdict: CONFIRMED
cases: executed 667→675, red 1
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 8 paths (listed in part 6)
needs-coordinator: the managed tree `spindle-w31-u4-adv-mutants` is finished but `worktree gc` kept it ("no-remote-recovery-proof"), so removing it is yours.

**Summary.** One acceptance case fails. If the issuer's key set no longer lists any key the build can use (an empty set, for example), the run treats that refresh as failed and keeps the old keys. The removed key keeps verifying for as long as the run lives. I also checked every other attack in the brief; those cases are green and kept as tests. I wrote 7 mutants of the implementation; the unit's 11 cases stayed green against every one, and my new cases fail on all 7.

## 1. `git --no-pager diff --stat 83f1df99 HEAD`
```
 .../tests/spindle-cli/adversary_w31_u4.rs          | 843 +++++++++++++++++++++
 crates/spindle-cli/tests/spindle-cli/main.rs       |   1 +
 2 files changed, 844 insertions(+)
```
Both paths are test files; no implementation file is touched. Commit e0e5b04d on `unit/w31-token`, made with `b10x-gates bot -- commit`. The privacy grep printed nothing.

## 2. Cases added
File: `<local path>`

**Red:** `a_key_removed_by_emptying_the_set_no_longer_verifies` (8.1 s). The key set starts with k1 and refreshes every 1 s; it then serves `{"keys": []}`, and the test expects k1's token to get 401 within 8 s. Run alone first (`cargo test --locked -p spindle-cli --test spindle-cli -- adversary_w31_u4::a_key_removed_by_emptying_the_set_no_longer_verifies --exact`):
```
panicked at crates/spindle-cli/tests/spindle-cli/adversary_w31_u4.rs:691:5:
a key the issuer removed from its set, leaving it empty, still verifies: ...
b10x-spindle: --caller-token .../caller-token.toml: the key set at http://127.0.0.1:34521/jwks could not be refreshed: it lists no signing key with a key id (kid) this build verifies; the keys fetched last still verify   (x8)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 699 filtered out; finished in 8.10s
```

**Green, kept as properties.** The mutant in brackets makes each one fail.
- `the_bearer_token_is_read_once_from_authorization_and_nowhere_else`: `bearer`, `BEARER` and `bEaReR` are accepted. These get 401: the token twice, a second Authorization header, `?access_token=`, a cookie, `Basic`, no scheme, `Bearer<token>` with no space, `Bearer` alone, `X-Access-Token`. [case-sensitive scheme; `once`→`headers.get`]
- `a_token_whose_header_or_claims_fail_is_refused_beyond_the_listed_cases`:
  - 401: no kid, HS256 keyed with the public point or the PEM, a swapped payload, no exp, iss or aud, nbf +600 s, exp as a string, an aud array without the audience, `aud: []`, iss with a trailing slash.
  - 200: an aud array that contains the audience.
  - 403: a role claim that is an array, a number, an object, or the wrong case. [`validate_nbf = false`]
- `the_mapping_leeway_bounds_exp_and_nbf`: leeway 120. exp −90 s → 200, exp −200 s → 401, nbf +90 s → 200, nbf +200 s → 401. [mapping leeway not applied: the library default of 60 then refuses exp −90]
- `a_key_for_encryption_or_for_another_algorithm_verifies_nothing`: a `use: enc` key and a P-256 key with `alg: ES384` get 401; a bare P-256 key gets 200. [the `use` check dropped; the key's `alg` check dropped]
- `a_refresh_that_fails_keeps_the_keys_and_warns_naming_the_url`: the set starts answering 500; a warning line names the URL; the token still gets 200; no token appears on stderr.
- `a_key_set_too_large_too_slow_or_without_a_usable_key_stops_the_run` (10.0 s, over 5 s): a body over 1 MiB, a set with no usable key, and a server that never answers each exit 1 with the URL named. [`JWKS_LIMIT` raised to 4 MiB]
- `a_mapping_out_of_bounds_exits_1_before_listening`: each of these exits 1 with a line naming `--caller-token`: leeway 301, refresh 0, refresh 86401, HS256 in the algorithms, a JWKS URL `http://192.0.2.1/...`, attributes for an actor no role maps to, and `--fixed-actor` given with the token.

Mutant runs were in the managed tree `spindle-w31-u4-adv-mutants`, never in the unit's tree:
- First pass, all 7 mutants together: `6 failed` (5 of my cases plus the red one); the unit's 8 `caller_token::` cases and 3 `token::tests` all PASS.
- Second pass, only the two mutants the first pass masked (the `once` and key-`alg` mutants): `3 failed`, with "the same valid token twice" and "a P-256 key naming ES384"; the unit's 11 cases PASS.

## 3. Suite, run after the cases existed
`SLOT_TIMEOUT=30m cargo test --locked -p spindle-cli --no-fail-fast` → exit 101
```
test result: ok. 8 passed; 0 failed; ...   (bin unit tests)
test result: FAILED. 663 passed; 4 failed; 33 ignored; 0 measured; 0 filtered out; finished in 30.48s
```
- The 4 failures are the 3 UI cases already red on the base plus my red case.
- "Before" (659 + 8) comes from the implementor's gate log `U4/scratch/gate/test-cli.log`: `656 passed; 3 failed; 33 ignored`.
- `cargo clippy --locked -p spindle-cli --all-targets -- -D warnings` → exit 0. `cargo fmt -p spindle-cli --check` → exit 0.

## 4. Findings
| # | file:line | verdict | origin | what was measured | what reaches it |
|---|---|---|---|---|---|
| 1 | crates/spindle-cli/src/token.rs:406 (with :555) | CONFIRMED | introduced | A 200 answer with an empty set, or with no usable key, counts as a failed refresh, so the old keys stay. The removed key keeps verifying, which breaks the literal acceptance "a removed key no longer does". The test above, line 691. | An issuer that revokes by emptying its set, or that rotates to keys this build skips (P-521, no kid). Possible fix: replace the keys with any set that parses as a JWKS, even an empty one; keep the old keys only when the fetch or the parse fails. This sets D5 against the acceptance, so the call is the coordinator's. |
| 2 | crates/spindle-ingress/src/listener.rs:679 | CONFIRMED | introduced | Read from the code, not tested: any token that verifies passes the gate, including one that maps to no caller (no role claim). With it, `/__spindle/requests`, `/__spindle/metrics` and `/__spindle/topology` answer, and the request log shows other callers' paths and queries. | Any token the issuer mints for this audience. It matches the letter of D4 B ("a verified token"), so this is a note. |
| 3 | crates/spindle-cli/src/token.rs:284 | CONFIRMED | introduced | The mapping is not checked against the model at start. A role mapped to an undeclared actor (`ghost` in the unit's own test mapping) or an attribute the actor does not declare starts the run, then answers 403 to every such token. | Any operator typo in a mapping file. |

## 5. Attacked and not broken
- `alg: none`, HS256 keyed with the public key, keys of another family: all refused.
- A kid not in the set, or no kid: refused.
- Wrong iss or aud, as a string or an array; exp and nbf just inside and just outside the leeway: handled as specified.
- A token in a query string or a cookie, two Authorization headers, any case of `Bearer`: handled as specified.
- 401 is not recorded or counted under `--observe`. The probes are GET only. The headerless router answers 401.
- D2, D3, D4 and D5 exit codes, D6 loopback-only http, the bounds on refresh_seconds and leeway, rotation without a restart.
- A huge, silent or unusable key set at start: the run exits 1 with the URL named. A failed refresh keeps the keys and writes a warning.
- `system: true` actor: no such concept exists in the repository or in ESS, so nothing to attack.
- iss as an array is accepted by jsonwebtoken (`Issuer::Multiple`), but only the issuer can sign such a token. I did not raise it.

## 6. Paths written outside the worktree
- `<local path>` (the directory), holding `red-alone.log`, `module.log`, `module-final.log`, `suite.log`, `mutants.log`, `mutants2.log`, `clippy.log` and `commit-msg.txt`
- `<local path>` (managed tree, cleaned and finished; gc kept it)

## 7.
```findings
- file: crates/spindle-cli/src/token.rs
  line: 406
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: a refreshed key set that lists no usable key (an empty set) counts as a failed refresh, so a key the issuer removed keeps verifying for the life of the run, against "a removed key no longer does"
- file: crates/spindle-ingress/src/listener.rs
  line: 679
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: a token that verifies but maps to no caller still reads /__spindle/requests, metrics and topology, so any holder of a token for the audience reads the operator's request log
- file: crates/spindle-cli/src/token.rs
  line: 284
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the mapping is not checked against the model at start, so a role mapped to an undeclared actor or an undeclared attribute starts the run and answers 403 to every such token
```
