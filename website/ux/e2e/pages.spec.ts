// L2, the layout contract, docs variant: every page of the sitemap × gating viewport × theme, read
// at its top and at its end: hard gate 1 (no horizontal scroll, nothing past its box), gate 2 (the
// always-visible elements fully in view) and gate 5 (axe-core, 0 serious or critical). Measured-only
// viewports are read too (`@measured`) and recorded without gating.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { adapter, origin } from '../adapter.ts';
import { judge } from '../core/gates.ts';
import type { Mark } from '../core/gates.ts';
import { checkPage, hermetic } from '../core/page.ts';
import { OUT } from './out.ts';

const marks = (JSON.parse(readFileSync(join(import.meta.dirname, '..', 'known-failing.json'), 'utf8')) as { marks: Mark[] }).marks;

test.beforeEach(async ({ context }) => hermetic(context, origin));

for (const site of adapter.pages) {
  for (const theme of adapter.themes) {
    for (const viewport of adapter.gating) {
      test(`L2 ${site.name} ${viewport.name} ${theme.name}`, async ({ page }) => {
        const run = await checkPage(page, adapter, site, viewport, theme, { axe: true, shots: join(OUT, 'shots') });
        mkdirSync(join(OUT, 'pages'), { recursive: true });
        writeFileSync(join(OUT, 'pages', `${site.name}--${viewport.name}--${theme.name}.json`), JSON.stringify(run, null, 1));
        const where = { scenario: site.name, viewport: viewport.name, theme: theme.name };
        const verdicts = [
          judge('gate-1', where, run.states.flatMap((s) => s.overflow.map((o) => `${s.state}: ${o.kind} ${o.what} past by ${o.by}px`)), marks),
          judge('gate-2', where, run.states.flatMap((s) => s.hidden.map((h) => `${s.state}: ${h.name} ${h.why}`)), marks),
          judge('gate-5', where, (run.axe ?? []).map((v) => `${v.impact} ${v.id} (${v.help}) at ${v.nodes.length} node(s): ${v.nodes.slice(0, 3).join(', ')}`), marks),
        ];
        if (run.status !== 200) verdicts.push(judge('gate-1', where, [`the page answered ${run.status}`], []));
        mkdirSync(join(OUT, 'verdicts'), { recursive: true });
        writeFileSync(join(OUT, 'verdicts', `L2--${site.name}--${viewport.name}--${theme.name}.json`), JSON.stringify(verdicts, null, 1));
        for (const v of verdicts) if (v.finding) test.info().annotations.push({ type: 'known-failing', description: v.message });
        expect(verdicts.filter((v) => !v.ok).map((v) => v.message)).toEqual([]);
      });
    }
    for (const viewport of adapter.measured) {
      test(`L2 ${site.name} ${viewport.name} ${theme.name} @measured`, async ({ page }) => {
        const run = await checkPage(page, adapter, site, viewport, theme, { axe: true });
        mkdirSync(join(OUT, 'measured'), { recursive: true });
        writeFileSync(join(OUT, 'measured', `${site.name}--${viewport.name}--${theme.name}.json`), JSON.stringify(run, null, 1));
      });
    }
  }
}
