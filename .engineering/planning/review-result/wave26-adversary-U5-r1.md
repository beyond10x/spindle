---
format: aep.planning-md/3
id: review-result:wave26-adversary-U5-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 26 U5
relations:
- reviews: story:release-linux-glibc-floor-and-musl
revision: 1
---
needs-revision

```
unit: story:release-linux-glibc-floor-and-musl
verdict: CONFIRMED (one finding); two gaps INFEASIBLE on this build; four release-run questions INCONCLUSIVE
cases: executed 0→32, red 3
origin: introduced 3 / pre-existing 0 / undecided 0
```

Confirmed: no task the repository runs tested `scripts/release-linux-binary.sh`; its only check was a
negative control inside the pull-request run of `release.yml`. Fixed in 89f2d6170: the adversary
test runs in `task check` (`check:the_release_binary_checks_refuse_what_they_must`).

Infeasible: the adversary found this build does not reach either; kept as notes:

- The glibc-floor check reads `objdump -T` symbol versions only, so a binary that needs
  `GLIBC_ABI_DT_RELR` (glibc 2.36, a version reference with no symbol) would pass; no packed relocations are asked for in the workflow or `.cargo/config.toml`.
- With `readelf` missing, the INTERP and NEEDED checks read empty output and the static mode passes on
  `file` alone; the runner image has binutils.

Inconclusive until a run: `cargo auditable zigbuild` on the runner, and what the tagged release shows
beyond the pull-request run.

```findings
- file: scripts/release-linux-binary.sh
  category: acceptance
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: no task the repository runs tested the script
- file: scripts/release-linux-binary.sh
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: the glibc floor reads symbol versions only and misses a version reference with no symbol
- file: scripts/release-linux-binary.sh
  category: boundary
  severity: note
  verdict: INFEASIBLE
  origin: introduced
  message: without readelf the INTERP and NEEDED checks read empty output
```
