---
format: aep.planning-md/3
id: review-result:wave31-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 31 U3
relations:
- reviews: story:listener-tls
revision: 1
---
```
unit: story:listener-tls, head 2887852b (adversary tests committed on top as 0989f370)
verdict: CONFIRMED
cases: executed 808→829, red 0
origin: introduced 7 / pre-existing 0 / undecided 0
wrote-outside-worktree: 17 paths (scratch logs, patches, commit message) plus one managed mutant worktree, now finished
needs-coordinator: none
```

I could not break the unit. Every attack in the brief passes on the unit head, so there are no red cases. What I did find is gaps in the unit's own tests: five one-line mutants of the implementation pass all of them. Each is now caught by a kept test of mine. The other two findings are doc notes about health probes.

**1. Diff stat (2887852b..0989f370)** — test files only, no implementation file touched:
```
 crates/spindle-cli/tests/spindle-cli/adversary_w31_u3.rs          | 976 +++
 crates/spindle-cli/tests/spindle-cli/main.rs                      |   1 +
 crates/spindle-ingress/tests/spindle-ingress/adversary_w31_u3.rs  | 513 +++
 crates/spindle-ingress/tests/spindle-ingress/main.rs              |   1 +
```

**2. Cases added** (all green now; the first run of each module was before any suite run)
- `spindle-ingress` `adversary_w31_u3`, 9 cases:
  - a plain HTTP request on the TLS port is closed within 3 s, with no HTTP answer;
  - 32 clients stalled in the handshake do not hold up a later client;
  - a stalled handshake is closed at `HEADER_READ`, between 9 s and 13 s. This one takes about 10 s, the only case over 5 s;
  - `close(30 s)` returns within 3 s with stalled handshakes open, and with an idle keep-alive TLS connection open;
  - `close` still writes an answer already handed back, and a request still queued at close gets 503, over TLS;
  - live and ready answer 200 over TLS, and ready answers 503 once stopping;
  - a surface requiring the proxy secret answers 403 without it and 200 with it, over TLS on `::1`;
  - a surface without the secret is refused `InvalidInput` off loopback (192.0.2.1).
- `spindle-cli` `adversary_w31_u3`, 12 cases:
  - one TLS flag without its pair exits 2 and names the missing flag (5 combinations);
  - TLS flags on a run that serves no HTTP are refused with "--tls-cert … would be ignored";
  - off loopback with mTLS but no trusted-caller header still exits 2;
  - a chained certificate file is served whole; a chain with the intermediate first exits 2 naming both files;
  - SEC1, P-384 and Ed25519 keys are served; one file holding certificate and key works for both flags;
  - an encrypted key, a directory, swapped files, and a key file given as the CA each exit 2 with `<flag> <file>: `;
  - with `--tls-client-ca`: an expired client certificate and one issued for servers only are refused; a client sending its intermediate, and one signed by the second CA in the file, are answered; ready is answered only with a client certificate;
  - TLS 1.2 and 1.3 are each negotiated and answered;
  - the proxy secret and console behave over TLS as over plain HTTP (403 without, 202 and 200 with);
  - SIGTERM with an idle TLS connection, a silent socket and half a handshake open exits 143, and the idle client sees the connection close.

Mutants, each applied in a separate managed worktree (never in the unit's tree), showing the unit's own tests (`tls::`, `listener_tls::`) stay green while a kept test goes red:
- **M1**, `listener.rs:595`, the drain arm of the handshake `select!` removed: `stalled_handshakes_do_not_hold_the_drain` failed with "the close waited 9.702384777s for stalled handshakes".
- **M2**, `listener.rs:589`, handshake timeout set to 3600 s: `a_stalled_handshake_is_closed_at_the_header_read_bound` failed with "a stalled handshake was still open after 20.168769904s".
- **M3**, `listener.rs:403`, `requiring_secret()` dropped from the TLS constructor: `a_surface_without_a_secret_is_not_bound_off_loopback_over_tls` failed its `left == right` assertion.
- **M4 and M5**, applied together:
  - M4, `listener_tls.rs:39`, protocol versions limited to TLS 1.3: `tls_1_2_and_1_3_are_both_served` failed with "TLS 1.2: received fatal alert: ProtocolVersion".
  - M5, `main.rs:876`, the "would be ignored" refusal on a run that serves no HTTP removed: `tls_flags_on_a_run_serving_no_http_are_refused` failed because the run exited 0 and silently ignored the flags.

**3. Suite runs (after the cases existed)**
- `cargo test --locked -p spindle-ingress --no-fail-fast`: exit 0, 20 s wall.
  - Integration binary `test result: ok. 178 passed; 0 failed`; lib `ok. 22 passed`.
- `cargo test --locked -p spindle-cli --no-fail-fast`: exit 101, 45 s wall.
  - `test result: FAILED. 648 passed; 3 failed; 33 ignored`.
  - The 3 failures are the known UI cases: `deploy::the_ui_is_served_from_the_binary` and the two `observe_panel::` cases.
- The "before" counts come from the filter counts of my module-only runs (169 and 672 filtered out): ingress 169→178, cli 639→651 executed. Lib tests are included in the header totals.
- `cargo clippy --locked -p spindle-ingress -p spindle-cli --all-targets -- -D warnings`: exit 0. `cargo fmt -p spindle-ingress -p spindle-cli --check`: exit 0.

**4. Findings** (they cover 2887852b)

| # | file:line | finding | verdict | origin | what reaches it |
|---|---|---|---|---|---|
| 1 | `crates/spindle-ingress/src/listener.rs:595` | The unit's tests miss the drain arm of the handshake (M1); now caught by a kept test. | CONFIRMED | introduced | `close` and SIGTERM while a client is stalled in the handshake |
| 2 | `crates/spindle-ingress/src/listener.rs:589` | The unit's tests miss the handshake bound (M2); now caught by a kept test. | CONFIRMED | introduced | any client that opens TCP and never finishes the handshake |
| 3 | `crates/spindle-ingress/src/listener.rs:403` | The unit's tests miss the off-loopback secret requirement of the TLS constructor (M3); now caught by a kept test. `main.rs` already refuses off loopback, so this is a second line of defence. | CONFIRMED | introduced | `bind_reading_on_tls` with a non-loopback IP |
| 4 | `crates/spindle-cli/src/listener_tls.rs:39` | Decision D2 (TLS 1.2 and 1.3) is not asserted by the unit's tests (M4); now caught by a kept test. | CONFIRMED | introduced | a proxy that offers only TLS 1.2 |
| 5 | `crates/spindle-cli/src/main.rs:876` | The new refusal on a run that serves no HTTP is untested (M5); without it the run exits 0 and ignores the flags. Now caught by a kept test. | CONFIRMED | introduced | `run ess --tls-cert … --tls-key …` |
| 6 | `website/docs/concepts/capabilities.md:133` | The page tells mTLS users to probe with `exec`. The published image (`Containerfile`: distroless, the binary alone) has no command an exec probe could run, and the binary has no probe subcommand. Only `tcpSocket` works. | CONFIRMED | introduced | a deploy of the published image with `--tls-client-ca` |
| 7 | `website/docs/concepts/observe.md:129` | The probe paragraph still says probes send the secret with `httpGet.httpHeaders`. With `--tls-client-ca` an `httpGet` probe cannot connect, and the page does not say so. | CONFIRMED | introduced | a reader of the health-route docs who runs mTLS |

**5. Attacked and not broken**
- The brief's attacks all held:
  - plain HTTP is closed at once;
  - a stalled handshake holds neither the accept loop nor the drain;
  - the clap pair rules hold;
  - chains, SEC1, P-384, Ed25519 and a combined file are served;
  - an encrypted key, a directory, swapped files and a mismatched key exit 2 naming the flag and file;
  - no client certificate, a certificate from another CA, an expired one, and one issued for servers only are each refused;
  - the proxy secret, console, live and ready behave the same over TLS;
  - the drain works with a TLS connection open;
  - TLS 1.2 and 1.3 are both served.
- The wasm targets do not pull in rustls or ring: `cargo tree --target wasm32-unknown-unknown -i rustls` and `-i ring` print nothing for spindle-wasm and spindle-node, and spindle-embed does not have the packages at all.
- The rustls key-match check works for all ring key types (checked in rustls 0.23.46 `crypto/ring/sign.rs`).
- Not attacked: PKCS#1 keys. No RSA key generator is available here (rcgen with ring cannot make one), and I did not want to commit a private key as a fixture.

**6. Paths written outside the worktree**
- `<local path>` containing:
  - logs: `cli-adv-1.log`, `cli-adv-2.log`, `clippy.log`, `fmt.log`, `ingress-adv-1.log`, `m1.log`, `m2.log`, `m3.log`, `m4-m5.log`, `suite-cli.log`, `suite-ingress.log`
  - patches: `m1.patch`, `m2.patch`, `m3.patch`, `m4-m5.patch`
  - `commit-msg.txt`
- The managed worktree `<local path>` (id `spindle-w31-tls-adv-mutant`): restored to clean, its 215 MiB `target/` discarded, and marked finished.

**7. Findings block**
```findings
- file: crates/spindle-ingress/src/listener.rs
  line: 595
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: removing the drain arm of the handshake select leaves the unit's tests green while close waits up to HEADER_READ for stalled handshakes; caught now by stalled_handshakes_do_not_hold_the_drain
- file: crates/spindle-ingress/src/listener.rs
  line: 589
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: an unbounded handshake timeout leaves the unit's tests green; caught now by a_stalled_handshake_is_closed_at_the_header_read_bound
- file: crates/spindle-ingress/src/listener.rs
  line: 403
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: dropping requiring_secret off loopback in bind_reading_on_tls leaves the unit's tests green; caught now by a_surface_without_a_secret_is_not_bound_off_loopback_over_tls
- file: crates/spindle-cli/src/listener_tls.rs
  line: 39
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: restricting the listener to TLS 1.3 against decision D2 leaves the unit's tests green; caught now by tls_1_2_and_1_3_are_both_served
- file: crates/spindle-cli/src/main.rs
  line: 876
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: without the new refusal a run serving no HTTP ignores the TLS flags and exits 0, and the unit's tests stay green; caught now by tls_flags_on_a_run_serving_no_http_are_refused
- file: website/docs/concepts/capabilities.md
  line: 133
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the mTLS probe advice names exec, but the published distroless image holds only the binary and the binary has no probe subcommand, so only tcpSocket is usable
- file: website/docs/concepts/observe.md
  line: 129
  category: contract-drift
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: the health-route page still describes httpGet probes with the secret header and does not say that an httpGet probe cannot connect under --tls-client-ca
```
