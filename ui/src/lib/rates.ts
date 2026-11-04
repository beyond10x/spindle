// Rates and quota shares from cumulative samples. A lost sample is harmless: every value is a
// difference between two samples that did arrive.

import type { Sample } from '../api/types';

const seconds = (at: string) => Date.parse(at) / 1000;

export const counter = (sample: Sample | undefined, name: string): number | undefined =>
  sample?.counters.find((c) => c.name === name)?.value;

/** Per second, between the last two samples; undefined until both report the counter. */
export function rate(samples: readonly Sample[], name: string): number | undefined {
  const last = samples[samples.length - 1];
  const before = samples[samples.length - 2];
  if (!last || !before) return undefined;
  const now = counter(last, name);
  const then = counter(before, name);
  const span = seconds(last.at) - seconds(before.at);
  if (now === undefined || then === undefined || !(span > 0)) return undefined;
  return (now - then) / span;
}

/** The rate of one counter, added over several instances; undefined when none reports it. */
export function sumRate(series: readonly (readonly Sample[])[], name: string): number | undefined {
  let total: number | undefined;
  for (const samples of series) {
    const one = rate(samples, name);
    if (one !== undefined) total = (total ?? 0) + one;
  }
  return total;
}

/** The latest value of a counter, added over several instances. */
export function sumCounter(series: readonly (readonly Sample[])[], name: string): number | undefined {
  let total: number | undefined;
  for (const samples of series) {
    const one = counter(samples[samples.length - 1], name);
    if (one !== undefined) total = (total ?? 0) + one;
  }
  return total;
}

const limited = (samples: readonly Sample[]) =>
  samples.length > 0 && samples[samples.length - 1]!.resources.cpu_share !== undefined;

/** CPU of quota per interval: Δ CPU seconds / (Δ wall seconds × vCPUs); null without a limit. */
export function cpuShares(samples: readonly Sample[], vcpus: number): number[] | null {
  if (!limited(samples) || !(vcpus > 0)) return null;
  const shares: number[] = [];
  for (let i = 1; i < samples.length; i++) {
    const a = samples[i - 1]!;
    const b = samples[i]!;
    const span = seconds(b.at) - seconds(a.at);
    if (span > 0) shares.push((b.resources.cpu_seconds - a.resources.cpu_seconds) / (span * vcpus));
  }
  return shares;
}

/** Memory of quota per sample; null without a limit. */
export function memoryShares(samples: readonly Sample[]): number[] | null {
  const last = samples[samples.length - 1];
  if (!last || last.resources.memory_share === undefined) return null;
  return samples.flatMap((s) => (s.resources.memory_share === undefined ? [] : [s.resources.memory_share]));
}

export const perSecond = (value: number | undefined): string =>
  value === undefined ? '–' : `${value.toFixed(1)}/s`;

export function bytes(value: number | undefined): string {
  if (value === undefined) return '–';
  if (value < 1024) return `${value} B`;
  if (value < 1048576) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / 1048576).toFixed(1)} MiB`;
}
