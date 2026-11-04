import { describe, expect, test } from 'vitest';
import { counter, cpuShares, memoryShares, rate, sumRate } from './rates';
import type { Sample } from '../api/types';
import { samples as recorded } from '../../dev/observe-fixtures';

const sample = (sequence: number, at: string, value: number, cpu: number, share?: number): Sample => ({
  instance_id: 'i-1',
  sequence,
  at,
  period_ms: 1000,
  counters: [{ name: 'timer.tick.fired', value }],
  histograms: [],
  recent: [],
  resources: {
    cpu_seconds: cpu,
    memory_bytes: 32 * 1048576,
    ...(share === undefined ? {} : { cpu_share: share, memory_share: 0.5 }),
  },
  dropped_frames: 0,
});

describe('rates from cumulative counters', () => {
  const samples = [
    sample(1, '2026-11-04T10:00:01.000Z', 1, 0.1, 0.1),
    sample(2, '2026-11-04T10:00:03.000Z', 5, 0.5, 0.17),
  ];

  test('a rate is the counter difference over the time between the last two samples', () => {
    expect(rate(samples, 'timer.tick.fired')).toBe(2);
    expect(counter(samples[1], 'timer.tick.fired')).toBe(5);
    expect(counter(samples[1], 'never.reported')).toBeUndefined();
    expect(rate(samples.slice(0, 1), 'timer.tick.fired')).toBeUndefined();
  });

  test('a rate summed over instances adds what each one reports', () => {
    expect(sumRate([samples, samples], 'timer.tick.fired')).toBe(4);
  });

  test('CPU of quota is the CPU time used per interval over interval times vCPUs', () => {
    expect(cpuShares(samples, 2)).toEqual([0.1]);
  });

  test('without a limit there is no share', () => {
    const free = [sample(1, '2026-11-04T10:00:01.000Z', 1, 0.1), sample(2, '2026-11-04T10:00:02.000Z', 2, 0.2)];
    expect(cpuShares(free, 1)).toBeNull();
    expect(memoryShares(free)).toBeNull();
    expect(memoryShares(samples)).toEqual([0.5, 0.5]);
  });

  test('the fixture ticks once a second', () => {
    expect(rate(recorded('rally-table-0'), 'timer.tick.fired')).toBe(1);
  });
});
