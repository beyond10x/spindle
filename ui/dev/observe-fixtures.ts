// The contract's fixtures (`ess/fixtures/observe/`), read from disk. Shared by the `pnpm dev`
// middleware and the tests, so both see exactly what the specification's fixtures hold.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Catalogue, Deployment, Sample, StateChange, ViewRow } from '../src/api/types.ts';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../ess/fixtures/observe');

const json = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const sorted = (dir: string) =>
  readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .sort();
const safe = (name: string) => /^[A-Za-z0-9._-]+$/.test(name) && !name.startsWith('.');

/** The raw document at a path below the root, for checks that must not trust its type. */
export const raw = (...path: string[]): unknown => json(join(root, ...path));

/** Every file below `dir`, relative to the root, in name order. */
export const files = (dir: string): string[] => sorted(join(root, dir)).map((name) => join(dir, name));

export const deployment = (): Deployment => json(join(root, 'deployment.json')) as Deployment;

export function catalogue(component: string): Catalogue | undefined {
  const path = join(root, 'catalogue', `${component}.json`);
  return safe(component) && existsSync(path) ? (json(path) as Catalogue) : undefined;
}

/** The recorded samples of one instance, in sequence order. */
export function samples(instanceId: string): Sample[] {
  const dir = join(root, 'samples', instanceId);
  if (!safe(instanceId) || !existsSync(dir)) return [];
  return sorted(dir)
    .map((name) => json(join(dir, name)) as Sample)
    .sort((a, b) => a.sequence - b.sequence);
}

/** Every recorded state change, in time order. */
export const states = (): StateChange[] =>
  files('state')
    .map((file) => raw(file) as StateChange)
    .sort((a, b) => a.at.localeCompare(b.at));

/**
 * View answers for the dev server. The contract does not specify view answers (they are the
 * application's own views), so this one file is the UI's: component → view → answer.
 */
export const views = (): Record<string, Record<string, ViewRow>> =>
  json(join(dirname(fileURLToPath(import.meta.url)), 'views.json')) as Record<
    string,
    Record<string, ViewRow>
  >;
