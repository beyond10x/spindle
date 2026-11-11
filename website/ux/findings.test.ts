// The findings test (SKILL.md § 5): every record in `ux/findings/` is well formed, no closed S1 or
// S2 record is without a regression asset that exists and names it, and known-failing marks name
// open findings only.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { problems, read } from './core/findings.ts';
import type { Mark } from './core/gates.ts';

const findings = read(join(import.meta.dirname, 'findings'));
const marks = (JSON.parse(readFileSync(join(import.meta.dirname, 'known-failing.json'), 'utf8')) as { marks: Mark[] }).marks;

test('no finding record breaks the rules', () => {
  assert.deepEqual(problems(findings, import.meta.dirname), []);
});

test('a gate is marked known-failing only with an open finding that exists', () => {
  const open = new Set(findings.filter((f) => f.header.status === 'open').map((f) => f.header.id));
  assert.deepEqual(marks.filter((m) => !open.has(m.finding)).map((m) => `${m.gate} ${m.scenario}: ${m.finding}`), []);
});
