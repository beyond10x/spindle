// The ratchet of SKILL.md § 6: every key lower-is-better, held against `baseline.json`, and the
// baseline itself held against the one on `main`. Keys are whatever a scenario carries on either
// side; a key on one side only is missing.

import { BAND_KEYS, STEP_KEYS, TOLERANCES, tolerance } from './metrics.ts';

export interface Entry {
  value: number;
  tolerance: number;
  decision?: string;
}

export interface Baseline {
  about: string;
  scenarios: Record<string, Record<string, Entry>>;
  tolerances: typeof TOLERANCES;
}

export type Measured = Record<string, Record<string, number>>;

export interface Violation {
  kind: 'worse' | 'improved-not-taken' | 'missing' | 'baseline-worse-without-decision';
  scenario: string;
  key: string;
  detail: string;
}

const EPSILON = 1e-9;

/** The largest value still within the entry's tolerance: steps and bands relative, counts absolute. */
export function allowed(key: string, entry: Entry): number {
  return STEP_KEYS.has(key) || BAND_KEYS.has(key) ? entry.value * (1 + entry.tolerance) : entry.value + entry.tolerance;
}

/** The smallest value that is not yet an improvement to take in: a band key's lower edge, else the value. */
export function lowest(key: string, entry: Entry): number {
  return BAND_KEYS.has(key) ? entry.value * (1 - entry.tolerance) : entry.value;
}

export function check(baseline: Baseline, measured: Measured, onMain: Baseline | null): Violation[] {
  const out: Violation[] = [];
  const scenarios = new Set([...Object.keys(baseline.scenarios), ...Object.keys(measured)]);
  for (const scenario of [...scenarios].sort()) {
    const base = baseline.scenarios[scenario] ?? {};
    const now = measured[scenario] ?? {};
    const keys = new Set([...Object.keys(base), ...Object.keys(now)]);
    for (const key of [...keys].sort()) {
      const entry = base[key];
      const value = now[key];
      if (entry === undefined) {
        out.push({ kind: 'missing', scenario, key, detail: 'no baseline value' });
        continue;
      }
      if (value === undefined || !Number.isFinite(value)) {
        out.push({ kind: 'missing', scenario, key, detail: 'no measured value' });
        continue;
      }
      if (value > allowed(key, entry) + EPSILON) {
        out.push({ kind: 'worse', scenario, key, detail: `${value} > ${entry.value} (tolerance ${entry.tolerance})` });
      } else if (value < lowest(key, entry) - EPSILON) {
        out.push({ kind: 'improved-not-taken', scenario, key, detail: `${value} < ${entry.value}: take it into the baseline in this merge` });
      }
      const main = onMain?.scenarios[scenario]?.[key];
      if (main !== undefined && entry.value > main.value + EPSILON && !entry.decision) {
        out.push({ kind: 'baseline-worse-without-decision', scenario, key, detail: `baseline ${entry.value} > main's ${main.value} and no decision id` });
      }
    }
  }
  return out;
}

/**
 * The baseline after taking the measured values in. A value worse than the old baseline needs a
 * decision id; without one the old value stays and the next `check` fails on it.
 */
export function update(baseline: Baseline, measured: Measured, decision?: string): Baseline {
  const scenarios: Baseline['scenarios'] = {};
  for (const scenario of Object.keys(measured).sort()) {
    const now = measured[scenario]!;
    const before = baseline.scenarios[scenario] ?? {};
    const next: Record<string, Entry> = {};
    for (const key of Object.keys(now).sort()) {
      const value = now[key]!;
      const old = before[key];
      const entry: Entry = { value, tolerance: tolerance(key) };
      const band = old !== undefined && BAND_KEYS.has(key) ? { ...old, tolerance: tolerance(key) } : undefined;
      if (band && value <= allowed(key, band) + EPSILON && value >= lowest(key, band) - EPSILON) {
        next[key] = band;
        continue;
      }
      if (old !== undefined && value > (band ? allowed(key, band) : old.value) + EPSILON) {
        if (!decision) {
          next[key] = old;
          continue;
        }
        entry.decision = decision;
      } else if (old?.decision && old.value === value) {
        entry.decision = old.decision;
      }
      next[key] = entry;
    }
    scenarios[scenario] = next;
  }
  return { about: baseline.about, scenarios, tolerances: TOLERANCES };
}
