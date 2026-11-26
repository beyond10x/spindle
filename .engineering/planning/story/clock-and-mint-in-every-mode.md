---
format: aep.planning-md/3
id: story:clock-and-mint-in-every-mode
kind: story
status: draft
title: Every run mode takes a simulated clock and a seeded mint
relations:
- serves: vision:spindle
revision: 1
---
# Every run mode takes a simulated clock and a seeded mint

## Outcome

`run` in each mode (stdin lines, HTTP, `--scenario`) takes `--mint-seed <n>`, so identities and
generated values are minted from that seed from the first command, and `--clock <RFC 3339>`, so the
run starts on a manual clock at that instant. On stdin a line may carry `"at": "<RFC 3339>"`, which
moves the clock to that instant before the command runs; an instant before the current one is
refused for that line. Periodic bindings fire as the clock passes their times, as `ManualClock`
drives them natively (`Host::advance`).

Today a seeded mint exists only behind `POST /api/reset` (`"mint": <seed>`), stdin mode mints random
UUIDs, and every mode reads the system clock. Reported 2026-11-26 by two consumers that simulate a
business over months: per-month figures need the clock under their control, and repeatable runs
need repeatable identities.

## Acceptance

- `a_seeded_stdin_run_mints_the_same_identities_twice`: two stdin runs with one `--mint-seed` print
  byte-identical results, minted identities included; a different seed mints different ones.
- `a_line_moves_the_clock`: with `--clock 2026-01-01T00:00:00Z`, a line with `"at":
  "2026-02-01T00:00:00Z"` runs a command whose guard orders against `now` at February 1st, and a
  later line with an earlier `at` is rejected.
- `a_periodic_binding_fires_as_the_clock_passes`: a monthly binding fires once per month line.
- `the_served_run_and_the_scenario_take_both_flags`: HTTP and `--scenario` runs honour
  `--mint-seed` and `--clock`; a reset world's `mint` still overrides the seed until the next reset.
