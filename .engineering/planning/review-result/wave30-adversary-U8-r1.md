---
format: aep.planning-md/3
id: review-result:wave30-adversary-U8-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U8
relations:
- reviews: story:peer-connect-test-steady
revision: 1
---
unit: wave 30 U8 story:peer-connect-test-steady, findings cover head 48e150dc (unit head 6d974673 plus adversary tests)
verdict: NEEDS-CHANGE
cases: executed 198→205, red 2
origin: introduced 2 / pre-existing 1 / undecided 0
wrote-outside-worktree: 6 paths (part 6)
needs-coordinator: none

1. Diff stat (6d974673..HEAD). Both paths are test files.
```
 .../tests/spindle-host/adversary_w30_u8.rs         | 237 +++++++++++++++++++++
 crates/spindle-host/tests/spindle-host/main.rs     |   2 +
 2 files changed, 239 insertions(+)
```
Commit 48e150dc is on `unit/w30-peer`. The module is `#[cfg(target_os = "linux")]`.

2. Cases added in `<local path>`:
- **RED** `the_peer_listens_at_the_port_a_refused_connection_reached`. It calls `Host::connect` to 127.0.0.1:P, where nothing listens, until the connection reaches itself and is refused. Then the peer binds P with `TcpListener::bind`, as `play --component` does. The bind fails. Red output from its first run, alone:
  ```
  panicked at crates/spindle-host/tests/spindle-host/adversary_w30_u8.rs:131:9:
  rally-table, started after rally-player-west's connection to 127.0.0.1:55550 reached itself and was refused, cannot listen there: Address already in use (os error 98)
  test result: FAILED. 0 passed; 1 failed; ... finished in 0.20s
  ```
- **RED** `a_refused_connection_closed_with_a_reset_leaves_its_port_free`. The stream is a self-connection with SO_LINGER 0 set before `link` gets it. The port is still blocked, because `link` runs `shutdown(Both)` first. This is the same finding seen from a second side. Output: `127.0.0.1:35423 after a reset: Address already in use (os error 98)`.
- GREEN `a_connection_to_itself_closed_with_a_reset_alone_frees_its_port`. Linger 0 and a close with no shutdown free the port at once. This supports the fix named below.
- GREEN `host_connect_refuses_the_connection_it_opened_to_itself`. This is acceptance 2 read literally, through `Host::connect` and not only through `link`. It catches a mutant where `connect` bypasses `link`'s check; the unit's `connect.rs` misses that mutant.
- GREEN `an_ipv6_connection_that_reached_itself_is_no_link` and `an_ipv4_mapped_connection_that_reached_itself_is_no_link`.
- GREEN `a_peer_on_the_unspecified_address_is_linked`. The peer is reached at both 0.0.0.0:P and 127.53.1.8:P, and no legitimate connection is refused.
- Every case runs in under 0.3 s.

3. Suite run, after the cases existed: `cargo test --locked -p spindle-host --no-fail-fast`, exit 101.
```
unittests: test result: ok. 1 passed; 0 failed
tests/spindle-host: test result: FAILED. 202 passed; 2 failed; 14 ignored; 0 measured; 0 filtered out; finished in 10.37s
  FAILED adversary_w30_u8::a_refused_connection_closed_with_a_reset_leaves_its_port_free
  FAILED adversary_w30_u8::the_peer_listens_at_the_port_a_refused_connection_reached
```
- 205 cases executed. The "before" count of 198 is 205 minus the 7 cases of `adversary_w30_u8`; I worked it out rather than running the suite again without them.
- `cargo clippy --locked -p spindle-host --all-targets -- -D warnings` passed (Finished). `rustfmt --check` on the module exit 0.

4. Findings
- **F1 (NEEDS-CHANGE, warning, introduced).** In `crates/spindle-host/src/host/component.rs:570`, `link` refuses a self-connection with `shutdown(Shutdown::Both)` and then drops the stream. The socket sends a FIN to itself and goes to TIME_WAIT on its own port. Linux keeps that state for 60 s, longer than `LINK_WITHIN` (30 s). A peer that starts later cannot bind that port, so the retry promised by the doc comment and by `CHANGELOG.md:169` ("the caller tries again until the peer listens") never succeeds.
  - A self-connection only happens when the peer is not listening yet, so the refusal path never leads to a link in any case where it fires. The failure moves from "own hello" to "the peer cannot listen".
  - What reaches it: `spindle-control/src/control.rs:159` `free_address`. It picks a port, drops it, and peers then connect on 127.0.0.1. `processes.rs` and `adversary_processes2.rs` do the same.
  - Fix: set SO_LINGER 0 and close without shutdown. The green premise case proves this works. `std`'s `set_linger` is unstable, so this needs rustix or socket2 as a normal dependency.
  - The unit's own tests on 127.53.0.n are not affected, because a self-connection cannot happen there.
- **F2 (CONFIRMED, note, pre-existing).** `free_addresses` at `processes.rs:60` and `adversary_processes2.rs:45` drop the port before the child binds it. Another process can take it in that gap: the HTTP failure in the notes. Because of F1, a self-connection there still fails the run after this change. Both files can fail the way the archived run did.
- **F3 (CONFIRMED, note, introduced).** `adversary_processes.rs:75` `host_of` uses 127.53.0.n. macOS lo0 has only 127.0.0.1 by default, so peer binds fail with EADDRNOTAVAIL on a macOS developer machine. The macOS CI lane runs only the `admit/conform/run/replay` modules of spindle-cli, so CI does not reach it. I did not run this on macOS.

5. Attacked and could not break:
- The check does not refuse legitimate IPv4, IPv6, IPv4-mapped, 0.0.0.0 or other-loopback connections.
- The CLI retry loop (`component.rs:150-157`) retries on every `Err`, the refusal included.
- On Linux, holding 127.0.0.1:P does not block a bind to 127.53.0.n:P; the unit's own test shows this. Ephemeral connects skip ports that are explicitly bound.
- Connects to 127.53.0.n leave from 127.0.0.1 (`ip route get`), so the unit's tests cannot self-connect.
- The structural check at `adversary_processes.rs:436` misses a case written as `#[test]\n#[ignore]\nfn`. That is minor and not raised as a finding.

6. Paths written outside the worktree:
- <local path>
- <local path>
- <local path>
- <local path>
- <local path>
- the directory <local path> itself

7.
```findings
- file: crates/spindle-host/src/host/component.rs
  line: 570
  category: acceptance
  severity: warning
  verdict: NEEDS-CHANGE
  origin: introduced
  message: link closes a refused self-connection with shutdown, leaving it in TIME_WAIT on its own port for 60 s, so the peer cannot bind there and the promised retry never links (fix - SO_LINGER 0, close without shutdown)
- file: crates/spindle-cli/tests/spindle-cli/processes.rs
  line: 60
  category: concurrency
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: free_addresses here and in adversary_processes2.rs:45 drop the port before the child binds it, so another process or a self-connection (now blocked by TIME_WAIT) can still fail these runs
- file: crates/spindle-cli/tests/spindle-cli/adversary_processes.rs
  line: 75
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: 127.53.0.n is not configured on macOS lo0 by default, so these cases fail with EADDRNOTAVAIL on a macOS developer machine, though no CI lane runs them there
```
