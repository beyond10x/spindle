---
format: aep.planning-md/3
id: review-result:wave31-acc-r4
kind: review-result
status: active
title: Plan critic (acceptance), round 4, on wave 31
relations:
- reviews: story:several-writers-one-postgres
revision: 1
---
needs-revision

**Finding status:** the round-2 finding is fixed in form. The test now times two events separately. One is the standby's own lease-taken line, within the lease plus one poll interval of the kill. The other is ready `200` within 10 s, with the open-stores-and-build-host step and the skipped seed named (D5 A). D7 A now names a standby poll interval of a third of the lease, so 2.67 s is derivable. The 10 s bound sits well inside the 60 s test limit. The Acceptance names the PostgreSQL lane, "(same)", for this test. The revision brings one new defect.

story:several-writers-one-postgres — the lease-taken line is bounded at exactly the theoretical worst case, 2.67 s, with no slack for query latency, process scheduling or the test's own stdout read, so the test fails on a loaded shared host even when the takeover is on time. The acceptance should state a bound with a named margin, or state 2.67 s as the product guarantee and the test's assertion as a wider number. — .engineering/planning/story/several-writers-one-postgres.md:108-110

With a 2 s lease renewed every 0.67 s, a SIGKILL just after a renew leaves the lease valid for about 2 s. A standby that polled just before expiry then needs a full poll interval, so the take happens at about 2.67 s plus latency. The asserted bound is the limit that this worst case already reaches.

**What I read:** the 1 id I was given, `story:several-writers-one-postgres`, whole (revision 16), via `aep plan artifact show`. I also read `git show 9fb7e949 -- .engineering/planning/story/`, `review-result:wave31-acc-r2`, the critic rubric and my role file.

**What I could not establish:**
- The text of the lease-taken line is not named in the story, only "prints its line". I did not raise it, because D5 A already names a log line and the test author can fix the text.
- Out of my lane (design): D3 A says a standby "opens no store", yet it must poll the lease from the database. The story does not say which connection does that.

```findings
- file: .engineering/planning/story/several-writers-one-postgres.md
  line: 108
  category: acceptance
  severity: warning
  verdict: needs-revision
  origin: introduced
  message: "a_killed_leader_leaves_the_standby_serving bounds the standby's lease-taken line at exactly lease plus one poll interval (2.67 s), the worst case of a 2 s lease polled at a third of it, with no margin for query latency, scheduling or reading the line, so the test fails on a loaded host with an on-time takeover; the acceptance needs a named margin on the assertion"
```
approve

**Finding status:** the round-3 finding is fixed.
- The product bound stays at 2.67 s, the lease plus one poll interval with the test's 2 s lease polled at a third of it.
- The test asserts 5 s, and the story states the 2.33 s margin and what it covers (query latency, scheduling, reading the line).
- The test prints the measured time, so the margin can be checked later.
- The line text is named: `took the lease of <component>`.
- The ready `200` bound of 10 s from the kill still holds. It is consistent with the 5 s bound, since the standby has to open the stores and build the host after the lease take.
- The test still fits within 60 s.

The Acceptance still names the PostgreSQL lane, "(same)", for this test.

D7 A now says the standby polls through one connection that reads and takes only the lease rows, which answers my out-of-lane design note against D3 A ("opens no store").

**New defects:** none. The Acceptance and D7 A do not contradict each other, and the Outcome's "lease time plus one renew interval" matches the poll interval (both a third of the lease).

**What I read:** the 1 id given, `story:several-writers-one-postgres`, via the diff of `git show 449500a9 -- .engineering/planning/story/` (revision 18) against my round-3 read of revision 16.

**What I could not establish:** the story does not say which line the test reads when a specification has several components, since each component has its own lease and one line per component. It does not matter for a single-component test, so I did not raise it.

```findings
[]
```
