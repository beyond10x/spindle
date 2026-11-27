---
format: aep.planning-md/3
id: review-result:wave30-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 30 U3
relations:
- reviews: story:unknown-instance-unsourced-unserved
- reviews: story:error-payload-generated-fields
revision: 1
---
I found no failing case. Both stories hold under every attack I could run; the one finding is a gap in the unit's tests, and my new tests now close it.

```
unit: story:unknown-instance-unsourced-unserved, story:error-payload-generated-fields; findings cover 447309ce, tests committed as 99b4c5de
verdict: CONFIRMED (one note: a gap in the unit's tests, no defect in the code)
cases: executed 437→441, red 0
origin: introduced 1 / pre-existing 0 / undecided 0
wrong-outside-worktree: see part 6
needs-coordinator: none
```

**1. Diff stat (447309ce..HEAD)** — every path is a test file or a test fixture:
```
 crates/spindle-kernel/tests/fixtures/adversary_w30_u3/domains/yard.yaml | 240 +
 crates/spindle-kernel/tests/fixtures/adversary_w30_u3/system.yaml      |  27 +
 crates/spindle-kernel/tests/spindle-kernel/adversary_w30_u3.rs         | 406 +
 crates/spindle-kernel/tests/spindle-kernel/main.rs                     |   1 +
```

**2. Cases added** in `<local path>`. All are green now and were green when first run alone, so there is no red output to quote.

The fixture is a bay that makes crates (Uuid identity, plus a generated Boolean on the event), slots (Integer identity) and tags (String identity). One run refuses requests along every path:
- `when:` refusals, with fields `{generated: true}`;
- `when:` refusals, with the same fields given no source;
- `wrong_state:` on a held row;
- `wrong_state:` on a row nobody holds;
- an unserved `unknown_instance:` refusal.

The error fields cover Integer, String, Timestamp, Boolean, Uuid, a record and a map.

- `every_refusal_path_and_error_field_type_draws_as_ess_draws`: under the tick mint, no error field draws from the run's mint. The crates equal `ess_conformance::interpret` 0.99.2. The slots and tags equal the same run without the refusals.
- `a_seeded_mint_keeps_every_creation_in_step_across_every_refusal_path`: seeds 0, 1, 7 and `u64::MAX`. Creations are the same with and without the refusals, and one seed gives the same error values on every run.
- `a_seeded_mint_answers_each_refusal_its_own_values`: the 6 refusals mint 6 different tickets.
- `an_unserved_unknown_instance_refusal_names_the_required_field_and_draws_nothing`: the message names `bay.yard.Missing.crate_id` and not the `Optional` `hint` declared before it. No tick is drawn and no row is written.

Timing: the 4 cases together took 0.01 s.

**Mutant probe**, in a separate managed tree at 447309ce:
- Record-member mutant, `mint.rs:347` changed `error` → `false`: `cargo test -p spindle-kernel` gave `424 passed; 2 failed`. The 2 failures are my 2 new tests; the unit's own tests all stayed green. Failure text: `no error field draws from the run's mint: ["bay.yard.Jammed.serial", "bay.yard.Jammed.rank", "bay.yard.Crate.crate_id", ...]`.
- Map-key mutant, `mint.rs:364` changed `error` → `false`: the same result, `424 passed; 2 failed`, only mine red.

**3. Suite run**, after the cases existed:
- `cargo test --locked -p spindle-kernel --no-fail-fast`: exit 0. Unit tests `15 passed`; integration tests `426 passed; 0 failed; 55 ignored`, finished in 11.10 s.
- `cargo clippy --locked -p spindle-kernel --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-kernel --check`: exit 0, after I ran rustfmt on my file.
- `executed 437→441` counts both kernel test binaries; the 4 added cases are the difference.

**4. Finding**
- `crates/spindle-kernel/src/mint.rs:347` and `:364`: the unit's tests never put a record or map field into an error. If a record member or map key of an error field were drawn from the run's mint, the error-draw tests would stay green. Verdict CONFIRMED, origin introduced, severity note.
- What reaches it: any `{generated: true}` or unsourced error field of a record or map type. The rule in `spindle_capability::mint::generated` accepts both.
- My committed tests now catch both mutants.

**5. What I attacked and could not break**
- Draw parity with the interpreter across several commands, all refusal paths, and Integer and String identities after a refusal.
- A Timestamp error field: it draws nothing.
- Seeded determinism and independence across seeds, including the `u64::MAX` seed.
- 501 per request and at admission: the kernel's `taking` check runs before anything is drawn. The CLI `admit`/`run` listing calls that same function.
- A scan of all 195 `system.yaml` specs in the repository, including `ess/` (the conformance suite) and `examples/rally`. Only `held-refusal`, `unknown-refusal` and my own fixture have an unserved `unknown_instance:` field, so conformance and the examples are unaffected.
- Not tested: a host binding chain that reaches the unserved outcome. No spec says what it should answer, and the unit changed the `notice` fixture so that path is no longer exercised.
- A `{generated: true}` enum in an error is refused by name. That is the existing rule (enums are response-only), not this unit's change.

**6. Paths written outside the worktree**
- `<local path>`, which holds `k1.log`, `k2.log`, `k3.log`, `probe.log`, `roots.txt`, `mut1.log`, `mut2.log`, `fmt.log`, `suite.log`, `clippy.log` and `commit-msg.txt`.
- `<local path>`: the archive `worktree gc` required before removing the mutant tree. The tree itself is removed.
- The temporary fixture-scan test was deleted before the commit.

**7.**
```findings
- file: crates/spindle-kernel/src/mint.rs
  line: 347
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: introduced
  message: passing false for a record member or map key of an error field (also mint.rs:364) leaves the unit's suite green (424 passed); only adversary_w30_u3's two draw-order cases catch it
```
