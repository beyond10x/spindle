import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { parse, problems } from './findings.ts';

const record = (over: Record<string, string>) => {
  const header: Record<string, string> = {
    id: 'UXF-001',
    title: 't',
    class: 'layout',
    severity: 'S2',
    metric: 'gate-1',
    status: 'open',
    found_by: 'L2',
    evidence_recording: 'none',
    evidence_screenshot: 'shots/x.png',
    evidence_lines: 'top',
    regression_asset: 'none',
    ...over,
  };
  return `---\n${Object.entries(header).map(([k, v]) => `${k}: ${v}`).join('\n')}\n---\n\nbody\n`;
};

test('a finding record reads its header fields', () => {
  const f = parse('UXF-001.md', record({}));
  assert.equal(f.header.id, 'UXF-001');
  assert.equal(f.header.regression_asset, 'none');
});

test('an open S2 without an asset passes, a closed one fails', () => {
  assert.deepEqual(problems([parse('UXF-001.md', record({}))], '.'), []);
  assert.deepEqual(problems([parse('UXF-001.md', record({ status: 'closed' }))], '.'), ['UXF-001.md: closed S2 without a regression asset']);
  assert.equal(problems([parse('UXF-001.md', record({ status: 'closed', severity: 'S1', regression_asset: '' }))], '.').length, 1);
});

test('a closed S3 without an asset passes', () => {
  assert.deepEqual(problems([parse('UXF-001.md', record({ status: 'closed', severity: 'S3' }))], '.'), []);
});

test('the asset of a closed record must exist and name the finding', () => {
  const out = join(import.meta.dirname, '..', '.out');
  mkdirSync(out, { recursive: true });
  const dir = mkdtempSync(join(out, 'findings-'));
  writeFileSync(join(dir, 'a.spec.ts'), "test('UXF-001 holds', () => {});\n");
  writeFileSync(join(dir, 'b.spec.ts'), "test('other', () => {});\n");
  const closed = (asset: string) => parse('UXF-001.md', record({ status: 'closed', regression_asset: asset }));
  assert.deepEqual(problems([closed('a.spec.ts::UXF-001 holds')], dir), []);
  assert.deepEqual(problems([closed('b.spec.ts')], dir), ['UXF-001.md: regression asset b.spec.ts does not name UXF-001']);
  assert.deepEqual(problems([closed('c.spec.ts')], dir), ['UXF-001.md: regression asset c.spec.ts does not exist']);
});

test('a missing field, an unknown class and an id that is not the file name fail', () => {
  const bad = parse('UXF-002.md', record({ class: 'pretty' }).replace('metric: gate-1\n', ''));
  assert.deepEqual(problems([bad], '.'), ['UXF-002.md: no metric', 'UXF-002.md: id UXF-001 is not the file name', 'UXF-002.md: class pretty']);
});
