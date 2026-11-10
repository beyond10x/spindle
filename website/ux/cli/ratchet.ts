// The ratchet gate (SKILL.md § 6): `ux/.out/measured.json` against `ux/baseline.json`, and that
// baseline against the one on `main` (through git). Exit 1 on a value worse than its tolerance, an
// improvement not taken in, a missing value, or a baseline made worse without a decision id.
//
//   node cli/ratchet.ts [--update] [--decision <id>] [--main <ref>] [--measured <file>]

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { TOLERANCES } from '../core/metrics.ts';
import { check, update } from '../core/ratchet.ts';
import type { Baseline, Measured } from '../core/ratchet.ts';

const ux = resolve(import.meta.dirname, '..');
const { values } = parseArgs({
  options: {
    update: { type: 'boolean', default: false },
    decision: { type: 'string' },
    main: { type: 'string' },
    measured: { type: 'string', default: join(ux, '.out', 'measured.json') },
  },
});

const baselinePath = join(ux, 'baseline.json');
const ABOUT =
  'UX ratchet of the documentation site (website/ux/). Scenarios: each findability task (screen-only reader, worst over the gating viewports and themes) and each page of the sitemap (page.<name>: axe serious and critical nodes per theme, elements past their box, always-visible misses, reading load). Every key lower-is-better. Tolerances: counts 0, glance_cost steps 5 % relative, layout_shift 0.01 absolute, keyboard.tabs a 30 % band both ways (Tab counts vary run to run). Rewrite with `task ux:docs:baseline`; a worse value needs DECISION=<id>.';

function onMain(): Baseline | null {
  const top = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: ux, encoding: 'utf8' }).trim();
  const path = relative(top, baselinePath);
  const refs = values.main ? [values.main] : ['main', 'origin/main'];
  for (const ref of refs) {
    try {
      return JSON.parse(execFileSync('git', ['show', `${ref}:${path}`], { cwd: ux, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] })) as Baseline;
    } catch {
      continue;
    }
  }
  console.log(`ratchet: no ${path} on ${refs.join(' or ')}; the baseline is not compared with main`);
  return null;
}

if (!existsSync(values.measured)) {
  console.error(`ratchet: ${values.measured} is missing; run the measurement first`);
  process.exit(1);
}
const measured = JSON.parse(readFileSync(values.measured, 'utf8')) as Measured;
let baseline: Baseline = existsSync(baselinePath) ? (JSON.parse(readFileSync(baselinePath, 'utf8')) as Baseline) : { about: ABOUT, scenarios: {}, tolerances: TOLERANCES };

if (values.update) {
  baseline = update({ ...baseline, about: ABOUT }, measured, values.decision);
  writeFileSync(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
  console.log(`ratchet: wrote ${relative(process.cwd(), baselinePath)}`);
}

const violations = check(baseline, measured, onMain());
for (const v of violations) console.log(`ratchet: ${v.kind} ${v.scenario} ${v.key}: ${v.detail}`);
const keys = Object.values(baseline.scenarios).reduce((n, s) => n + Object.keys(s).length, 0);
console.log(`ratchet: ${Object.keys(measured).length} scenarios, ${keys} baseline values, ${violations.length} violations`);
process.exit(violations.length ? 1 : 0);
