// Groupings of a sample's counters by the name scheme of docs/design/observe.md § 2.

import type { Sample } from '../api/types';
import { counter, rate } from './rates';

export interface Row {
  name: string;
  count: number | undefined;
  rate: number | undefined;
}

const last = (samples: readonly Sample[]) => samples[samples.length - 1];

const names = (samples: readonly Sample[], declared: readonly string[] | undefined) =>
  declared ?? last(samples)?.counters.map((c) => c.name) ?? [];

/** One row per counter whose name starts with `prefix`; a declared counter never reported has no count. */
export function rows(
  samples: readonly Sample[],
  prefix: string,
  declared?: readonly string[],
): Row[] {
  return names(samples, declared)
    .filter((name) => name.startsWith(prefix))
    .map((name) => ({ name, count: counter(last(samples), name), rate: rate(samples, name) }));
}

/** Per command, the count and rate over all its outcomes. */
export function commandTotals(samples: readonly Sample[], commands: readonly string[]): Row[] {
  return commands.map((command) => {
    const outcomes = rows(samples, `command.${command}.`);
    const reported = outcomes.filter((o) => o.count !== undefined);
    const rated = outcomes.filter((o) => o.rate !== undefined);
    return {
      name: command,
      count: reported.length ? reported.reduce((n, o) => n + (o.count ?? 0), 0) : undefined,
      rate: rated.length ? rated.reduce((n, o) => n + (o.rate ?? 0), 0) : undefined,
    };
  });
}

/** `binding.<b>.<kind>` or `timer.<b>.<kind>` grouped by binding: binding → kind → row. */
export function byBinding(
  samples: readonly Sample[],
  prefix: 'binding.' | 'timer.',
  declared?: readonly string[],
): Map<string, Map<string, Row>> {
  const out = new Map<string, Map<string, Row>>();
  for (const row of rows(samples, prefix, declared)) {
    const rest = row.name.slice(prefix.length);
    const dot = rest.lastIndexOf('.');
    const binding = rest.slice(0, dot);
    const kind = rest.slice(dot + 1);
    if (!out.has(binding)) out.set(binding, new Map());
    out.get(binding)!.set(kind, row);
  }
  return out;
}
