// Finding records (SKILL.md § 5): `UXF-nnn.md`, a header of `key: value` lines between `---`
// fences, then free text. No S1 or S2 finding closes without a regression asset.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export const HEADER_FIELDS = [
  'id',
  'title',
  'class',
  'severity',
  'metric',
  'status',
  'found_by',
  'evidence_recording',
  'evidence_screenshot',
  'evidence_lines',
  'regression_asset',
] as const;

export const CLASSES = ['layout', 'cue', 'affordance', 'copy', 'noise', 'engine', 'input'] as const;
export const SEVERITIES = ['S1', 'S2', 'S3'] as const;
export const STATUSES = ['open', 'closed', 'known-loss'] as const;

export type Header = Record<(typeof HEADER_FIELDS)[number], string>;

export interface Finding {
  file: string;
  header: Partial<Header>;
}

const NONE = new Set(['', 'none', '-', '–', 'n/a']);

export function parse(file: string, text: string): Finding {
  const header: Partial<Header> = {};
  const match = /^---\n([\s\S]*?)\n---/.exec(text);
  for (const line of (match?.[1] ?? '').split('\n')) {
    const colon = line.indexOf(':');
    if (colon < 0) continue;
    const key = line.slice(0, colon).trim() as keyof Header;
    if ((HEADER_FIELDS as readonly string[]).includes(key)) header[key] = line.slice(colon + 1).trim();
  }
  return { file, header };
}

export function read(dir: string): Finding[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((name) => /^UXF-\d{3}\.md$/.test(name))
    .sort()
    .map((name) => parse(name, readFileSync(join(dir, name), 'utf8')));
}

/**
 * Every problem of the records. `root` resolves a regression asset `path` or `path::name`; a
 * closed S1 or S2 record needs one that exists and names the finding.
 */
export function problems(findings: readonly Finding[], root: string): string[] {
  const out: string[] = [];
  for (const { file, header } of findings) {
    for (const field of HEADER_FIELDS) if (header[field] === undefined) out.push(`${file}: no ${field}`);
    if (header.id && `${header.id}.md` !== file) out.push(`${file}: id ${header.id} is not the file name`);
    if (header.class && !(CLASSES as readonly string[]).includes(header.class)) out.push(`${file}: class ${header.class}`);
    if (header.severity && !(SEVERITIES as readonly string[]).includes(header.severity)) {
      out.push(`${file}: severity ${header.severity}`);
    }
    if (header.status && !(STATUSES as readonly string[]).includes(header.status)) out.push(`${file}: status ${header.status}`);
    const serious = header.severity === 'S1' || header.severity === 'S2';
    if (header.status !== 'closed' || !serious) continue;
    const asset = header.regression_asset ?? '';
    if (NONE.has(asset)) {
      out.push(`${file}: closed ${header.severity} without a regression asset`);
      continue;
    }
    const [path] = asset.split('::');
    const full = resolve(root, path ?? '');
    if (!existsSync(full)) out.push(`${file}: regression asset ${path} does not exist`);
    else if (header.id && !readFileSync(full, 'utf8').includes(header.id)) {
      out.push(`${file}: regression asset ${path} does not name ${header.id}`);
    }
  }
  return out;
}
