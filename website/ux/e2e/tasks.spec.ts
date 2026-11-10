// L3, the screen-only reader: every findability task × gating viewport × theme. Gate 4: the task
// reaches its truth by pointer. Gate 5's keyboard half: the same task by keyboard alone.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { adapter, origin } from '../adapter.ts';
import { play } from '../core/bot.ts';
import type { Mode } from '../core/bot.ts';
import { judge } from '../core/gates.ts';
import type { Mark } from '../core/gates.ts';
import { hermetic } from '../core/page.ts';
import { OUT } from './out.ts';

const marks = (JSON.parse(readFileSync(join(import.meta.dirname, '..', 'known-failing.json'), 'utf8')) as { marks: Mark[] }).marks;

test.beforeEach(async ({ context }) => hermetic(context, origin));

for (const task of adapter.tasks) {
  for (const theme of adapter.themes) {
    for (const viewport of adapter.gating) {
      for (const mode of ['pointer', 'keyboard'] as Mode[]) {
        test(`L3 ${task.id} ${viewport.name} ${theme.name} ${mode}`, async ({ page }) => {
          const run = await play(page, adapter, task, viewport, theme, mode);
          const id = `${task.id}--${viewport.name}--${theme.name}--${mode}`;
          mkdirSync(join(OUT, 'plays'), { recursive: true });
          writeFileSync(join(OUT, 'plays', `${id}.json`), JSON.stringify(run, null, 1));
          const where = { scenario: task.id, viewport: viewport.name, theme: theme.name };
          const failures = run.reached ? [] : [`"${task.question}" not reached by ${mode}: ${run.stuck ?? 'no reason'}; path: ${run.path.join(' → ') || '(none)'}`];
          const verdict = judge(mode === 'pointer' ? 'gate-4' : 'gate-5-keyboard', where, failures, marks);
          mkdirSync(join(OUT, 'verdicts'), { recursive: true });
          writeFileSync(join(OUT, 'verdicts', `L3--${id}.json`), JSON.stringify([verdict], null, 1));
          if (verdict.finding) test.info().annotations.push({ type: 'known-failing', description: verdict.message });
          expect(verdict.ok, verdict.message).toBe(true);
        });
      }
    }
  }
}
