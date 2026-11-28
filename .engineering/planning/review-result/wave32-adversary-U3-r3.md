---
format: aep.planning-md/3
id: review-result:wave32-adversary-U3-r3
kind: review-result
status: active
title: Adversary, round 3, on wave 32 U3
relations:
- reviews: story:leader-exits-promptly-on-sigterm
revision: 1
---
unit: story:leader-exits-promptly-on-sigterm (U3) round 3; the findings cover head d7cbfdf1d (fix 87ba5925c plus my test commit)
verdict: REFUTED (no break reachable under the story's conditions; one note on a damaged doc comment)
cases: executed 152→153 in the PostgreSQL lane; spindle-kernel test binary 512 run, 61→62 ignored; spindle-ingress test binary 187 run; red 0
origin: introduced 1 / pre-existing 0 / undecided 0
wrote-outside-worktree: 7 new scratch files (part 6)
needs-coordinator: the three probe worktrees from rounds 1 and 2 are still registered, and removing them is yours

The new rule holds on both points you asked about. A read that completes in the instant the flag is set is answered normally, and an empty page from a given-up egress read cannot be taken for "no events" after a restart. All my cases pass, including the round-2 one that was red. The only finding is a doc comment in `sqlite.rs` that round 3 damaged.

My earlier files are unchanged: `git diff b7b443825 87ba5925c -- '*adversary_w32_u3*'` prints 0 lines.

**1. Diff** (`git diff --stat 87ba5925c..HEAD`): one test file.
```
 crates/spindle-kernel/tests/spindle-kernel/adversary_w32_u3.rs | 94 +++
```

**2. Case added** (green, kept): `adversary_w32_u3::postgres::a_read_asked_with_the_flag_set_completes_unless_it_has_to_wait` (3.1 s alone, 8.4 s in the lane).
- With the stop flag set before the read and nothing to wait for, `events_after_unless_stopped` and `events_head_unless_stopped` both return `Some`.
- Once a write of the store is withheld behind another session's open transaction, both return `None` within 0.5 s.
- Alone it passed (`adv-scratch/r3-case-alone.out`).
- In the first full lane run it failed on my own setup: transactions from other lane tests held the feed back, so the "nothing to wait for" half did have to wait and got `None`. That is the rule working as written, not a defect.
- I fixed the setup: the test now first reads once without the flag, which waits until the feed answers the write. The second lane run was green.
- It runs over 5 s in the lane because that first read waits for other tests' transactions.

**3. Suite** (`adv-scratch/r3-suite.sh`; logs `r3-suite.out` and `r3-lane-2.out`):
- `slot --timeout 30m --tokens 4 task check:postgres`, second run: exit 0. `153 tests run: 153 passed (1 slow)`. All six of my lane cases passed:
  - `a_leader_with_an_events_read_under_way_exits_within_the_bound_under_an_open_transaction` (round 1, red then, green now)
  - `a_post_under_way_at_sigterm_is_saved_before_the_standby_takes_over`
  - `a_renewal_after_a_release_that_waited_out_its_lock_waits_a_third_of_the_lease_again`
  - `a_standby_that_leads_exits_within_the_bound_with_an_events_read_under_way`
  - `a_view_read_waiting_for_a_slot_behind_an_events_read_does_not_hold_the_exit`
  - the new case above
- `cargo test --locked -p spindle-kernel -p spindle-ingress --no-fail-fast`: exit 0. Kernel `512 passed; 62 ignored`; ingress `187 passed`.
  - The round-2 red case `an_events_read_under_way_that_read_every_event_is_answered_with_them_after_the_signal` is now `ok`.
  - `a_sqlite_read_with_its_stop_flag_down_answers_as_the_read_without_one` is `ok`.
- `cargo clippy` on the three crates: exit 0. `cargo fmt --check`: exit 0.

**4. Findings**
- **Damaged doc comment** (`crates/spindle-kernel/src/sqlite.rs:1030`, note, CONFIRMED, introduced). Round 3 pasted code text into the doc comment of `offset_before`, which now reads:
  ```
  /// from where the last call         loop {
  // The feed answers a prefixped, so keeping one dead letter after another does not read
  ```
  It compiles, but the rendered doc is broken, and the second line is now a plain comment rather than doc text.
  - What reaches it: rustdoc and anyone reading the code. Behaviour is unchanged.

**5. Attacked and could not break**
- **A read that completes in the instant the flag is set:**
  - `published` checks the flag only after a feed ask that came back empty, and `take_unless` checks whether it is this read's turn before it checks the flag.
  - So a read whose ask was answered, or whose turn came, always completes. Only a read that is still waiting returns `None`, which maps to `503`.
  - My round-2 ingress case and the new kernel case pin both sides.
- **An empty page from a given-up egress read mistaken for "no events":**
  - On an empty page the lane only waits and leaves. Its cursor does not move and nothing is handed back to be saved.
  - A restart or reset reads the saved cursor again (`egress_position` at launch), so an empty page leaves no trace.
  - Since a free turn is now taken even with the flag set, a stopped lane may still get a full page. It posts none of it, because the stop is checked before each post, and an advance past unbound events stays local and is never saved.
  - Without a stop flag, `placed_after` never returns `None`.

**6. Paths written outside the worktree**
- New scratch files in `<local path>`: `r3-case-alone.out`, `r3-suite.sh`, `r3-suite.out`, `r3-lane-2.out`, `commit-msg-r3.txt`, `r2-suite.sh`, `commit-msg-r2.txt`.
- No new worktrees this round.

**7. Findings block**
```findings
- file: crates/spindle-kernel/src/sqlite.rs
  line: 1030
  category: judgement
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: round 3 pasted code text into the doc comment of offset_before, which now reads "from where the last call         loop {" followed by a plain comment line "The feed answers a prefixped"
```
