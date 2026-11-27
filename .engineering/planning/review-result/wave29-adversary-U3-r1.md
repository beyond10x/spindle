---
format: aep.planning-md/3
id: review-result:wave29-adversary-U3-r1
kind: review-result
status: active
title: Adversary, round 1, on wave 29 U3
relations:
- reviews: story:ess-pin-0-99-2
revision: 1
---
Nothing in the unit broke. The implementor's `List<String>` claim, the CI checksum and every pin file all check out, and the host gives the same answers as ESS's interpreter on 11 new quantifier shapes. One small gap in the pin check was already there at the base.

```
unit: story:ess-pin-0-99-2, head 71cc17b1 (unit head 9d122dd0 plus my one test commit)
verdict: REFUTED
cases: executed 70→82, red 0
origin: introduced 0 / pre-existing 1 / undecided 0
wrote-outside-worktree: 1 path (the scratch directory below)
needs-coordinator: none
```

**1. Diff of my commit** (`git diff --stat 9d122dd0..HEAD`): 24 files, 1006 insertions, all test paths:
- `crates/spindle-conformance/tests/spindle-conformance/adversary_w29_u3.rs` (new)
- `crates/spindle-conformance/tests/spindle-conformance/main.rs` (one line, `mod adversary_w29_u3;`)
- 22 fixture files under `crates/spindle-conformance/tests/fixtures/adversary-w29-u3/` (11 directories)

No non-test path was touched.

**2. Cases added** (all green, 12 passed in 0.01 s alone; none over 5 s)

Eleven cases each synthesise a fixture in process, run it on ESS's interpreter and on the host, and assert that both scenarios run, both give the same status, and both pass. The shapes:
- `forall-not`: a negated `forall` over a struct list
- `starts`, `inlist`, `ignorecase`: `trimmed` with `starts_with`, `in` and `equals_ignore_case`
- `inner`: the guarded text sits in a struct inside the list element
- `nested-str`, `nested-struct`: a quantifier over a list inside a list element
- `two-members`: an `all` over two members of the element
- `optional-label-list`: `Optional<List<Label>>`
- `string-list`, `optional-string-list`: `List<String>` and `Optional<List<String>>`

Using the `ess` CLI, all eight struct-element shapes and `Optional<List<Label>>` synthesise a different suite on 0.99.2 than on 0.99.1. Both `String`-list fixtures and an `equals` without `trimmed` synthesise the same bytes on both versions.

The twelfth case, `every_manifest_lock_entry_and_generated_header_names_the_required_ess_release`, compares against the release `ess/ess-inputs.yaml` requires:
- the 7 ESS lines of `Cargo.toml` and the `schema-contract` line in each of the two crate manifests;
- all 9 ESS sources in `Cargo.lock`;
- the `@generated` headers of `types.rs` and `types.ts`;
- `generator_version` in both `types-report.json`.

**3. Commands run**
- `cargo test --locked -p spindle-conformance --test spindle-conformance adversary_w29_u3`: 12 passed, 0 failed, 71 filtered out.
- `SLOT_TIMEOUT=30m cargo test --locked -p spindle-conformance --no-fail-fast`: 82 passed, 0 failed, 1 ignored, in 7.11 s; exit 0.
- The same run with `--skip adversary_w29_u3`: 70 passed, 1 ignored, 12 filtered out; exit 0. That is where the "70" comes from.
- `cargo clippy --locked -p spindle-conformance --all-targets -- -D warnings`: exit 0.
- `cargo fmt -p spindle-conformance -- --check`: exit 0, after I reformatted one `assert_eq!` in my own file.
- `slot ess generate --path ess --kind schema … --check`: "105 artifact(s), current".
- `slot sh scripts/check-ess-pin.sh`: "ess 0.99.2 in …"; exit 0.

**4. Findings**
- `scripts/check-ess-pin.sh:37-39`, CONFIRMED, pre-existing, note. The check reads only `ess-inputs.yaml`, `Taskfile.yml`, `.cargo/config.toml` and `ci.yml`. Reverting one manifest's ESS tag together with its lock entry to 0.99.1 would still pass `check:the_ess_pin_is_one_release` and `--locked`. What reaches it is a partial pin move by hand. My kept pin test now catches that.

**5. Claims checked and attacks that did not break anything**
- **`List<String>` claim holds.** On 0.99.1 and 0.99.2 the `List<String>` and `Optional<List<String>>` fixtures synthesise byte-identical suites. The reason: the accepting step already sends a one-element non-empty list (`["\u00a0\u00a0"]`), and the `\v\f\r\n` witness is the only element. So "two or more elements" cannot be met for `List<String>` on 0.99.2, and the switch to a struct list in the fixture is justified. The story's acceptance text still names `List<String>`.
- **Unit fixture changes as claimed.** On 0.99.1 its accepting step sent only `[]`, and the `\v\f\r\n` witness sat on element 0 next to the deciding `" \t "`. On 0.99.2 the accepting step sends three elements and the witness sits on the deciding element. Both unit tests would be red on 0.99.1.
- **CI checksum is right.** `8b32cb02…dc42e` equals the published SHA256SUMS line and the asset digest for `ess-0.99.2-x86_64-unknown-linux-gnu.tar.gz`. The tag `0.99.2^{}` points to `8b2da1d6…`, the commit `Cargo.lock` names.
- **Every pin file names 0.99.2.** The only remaining `0.99.1` mentions are:
  - Rust comments that name the interpreter release a rule was read from;
  - the stub Taskfiles in `scripts/wasm-cache-key.sh` and `scripts/adversary_w27_u6.sh`, which the brief says to leave;
  - the `producer` field of `ess/generated/jsonschema/.ess-output/state.json`, which the drift check accepts.
- **No other suite moves.** All 175 model directories of the base synthesise the same bytes on 0.99.1 (base tree) and 0.99.2 (head tree), `ess/` included. `conformance.toml` correctly stays as it is.
- **Nothing else in ESS 0.99.2 reaches Spindle.** Outside `crates/ui/*` and `synthesize.rs`, its `Cargo.toml` and `Cargo.lock` change only version strings, and `Cargo.lock` holds no `ess-ui` package.

**6. Paths written outside the worktree**
- `<local path>`: probe scripts (`variants.sh`, `variants2.sh`, `allmodels.sh`), `fx/`, `fx2/`, `all/`, `ess-base/`, `basetree/` (a base-tree archive), `rel/SHA256SUMS`, the `*.log` files and `commit-msg.txt`.

**7.**
```findings
- file: scripts/check-ess-pin.sh
  line: 37
  category: mutant
  severity: note
  verdict: CONFIRMED
  origin: pre-existing
  message: check:the_ess_pin_is_one_release reads no Cargo manifest or Cargo.lock, so one crate's ESS tag reverted together with its lock entry passes the gate; adversary_w29_u3 now covers it
```
