// The lint rule of SKILL.md § 1: the generic core imports nothing from the site or the adapter.

import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

const ALLOWED = [/^\.\/[^/]+\.ts$/, /^node:/, /^@playwright\/test$/, /^@axe-core\/playwright$/];

export function imports(source: string): string[] {
  const found: string[] = [];
  const pattern = /(?:^|\n)\s*(?:import|export)\s[^'"]*?from\s+['"]([^'"]+)['"]|import\(\s*['"]([^'"]+)['"]\s*\)|(?:^|\n)\s*import\s+['"]([^'"]+)['"]/g;
  for (const m of source.matchAll(pattern)) found.push((m[1] ?? m[2] ?? m[3])!);
  return found;
}

export const foreign = (specifiers: readonly string[]): string[] => specifiers.filter((s) => !ALLOWED.some((a) => a.test(s)));

const dir = import.meta.dirname;
const files = readdirSync(dir).filter((f) => f.endsWith('.ts'));

test('the core has files to check', () => {
  assert.ok(files.length > 5);
});

for (const file of files) {
  test(`core/${file} imports only the core, node and the browser tools`, () => {
    assert.deepEqual(foreign(imports(readFileSync(join(dir, file), 'utf8'))), []);
  });
}

test('the lint refuses an import of the adapter, the site or a framework', () => {
  const bad = `IMPORT { adapter } from '../adapter.ts';\nIMPORT config from '../../docusaurus.config.ts';\nIMPORT React from 'react';\nconst x = await IMPORT('../harness/server.ts');\n`.replaceAll('IMPORT', 'import');
  assert.deepEqual(foreign(imports(bad)), ['../adapter.ts', '../../docusaurus.config.ts', 'react', '../harness/server.ts']);
});
