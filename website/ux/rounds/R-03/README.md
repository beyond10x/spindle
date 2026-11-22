# R-03: operator notes

- Base: `origin/main` at 8591d014, 0.18.0 plus the ESS 0.97.4 pin, with the fourth unslop pass on
  `README.md` and three concept pages (`environment-and-admission`, `specification`, `observe`)
  uncommitted. R-02 ran on 82faeb87; `main` changed `website/docs` in between (the run options
  `--address`, `--trusted-caller-header`, `--trusted-caller-secret`, outbound HTTP over TLS, the
  supplied conformance suite, the PostgreSQL pages, the releases after 0.9.0 up to 0.18.0). Page keys
  therefore measure `main`'s new content as well as the unslop pass.
- `task ux:docs` is not part of CI, so the gate did not run on those merges. The measure found
  three regressions from `main`, not from the unslop pass: UXF-018's test (Getting started still
  showed 0.10.0), `request-log-size` at 1280x800 (UXF-022) and the `release` keyboard plays.
- No frozen build of the measured site: `website/build` was rebuilt for the change set. The measure
  log is the evidence of the red state (10 failed of 225, Playwright).
- Cards: anchors A-newcomer and B-integrator; no new rotating card, R02-phone-operator stays
  in rotation, since its task regressed.
- People: none; the cards are played by the screen-only reader (L3). No recordings.
- Not run: L4 and L5 (paid, no keys), L6 and L8 (no people).
