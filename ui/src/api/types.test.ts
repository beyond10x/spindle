// @vitest-environment node
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { describe, expect, test } from 'vitest';
import { catalogueShape, deploymentShape, sampleShape, stateChangeShape } from './shapes';
import { conform } from '../lib/conform';
import type { Catalogue, Deployment, Sample } from './types';

const fixtures = fileURLToPath(new URL('../fixtures/', import.meta.url));
const read = (name: string): unknown => JSON.parse(readFileSync(join(fixtures, name), 'utf8'));
const named = (prefix: string) =>
  readdirSync(fixtures)
    .filter((name) => name.startsWith(prefix) && name.endsWith('.json'))
    .sort();

describe('the_ui_types_match_the_api', () => {
  test('the deployment fixture has exactly the fields of Deployment', () => {
    expect(conform(read('deployment.json'), deploymentShape)).toEqual([]);
  });

  test('every catalogue fixture has exactly the fields of Catalogue', () => {
    const files = named('catalogue.');
    expect(files).toHaveLength(3);
    for (const file of files) expect(conform(read(file), catalogueShape), file).toEqual([]);
  });

  test('every sample fixture has exactly the fields of Sample', () => {
    const files = named('samples.');
    expect(files).toHaveLength(3);
    for (const file of files) {
      const samples = read(file) as unknown[];
      expect(samples, file).toHaveLength(30);
      samples.forEach((sample, index) =>
        expect(conform(sample, sampleShape), `${file}[${index}]`).toEqual([]),
      );
    }
  });

  test('every state change fixture has exactly the fields of StateChange', () => {
    const changes = read('states.json') as unknown[];
    expect(changes.length).toBeGreaterThan(0);
    changes.forEach((change, index) =>
      expect(conform(change, stateChangeShape), `states.json[${index}]`).toEqual([]),
    );
  });

  test('a field the type does not declare is refused, and so is a missing one', () => {
    const deployment = read('deployment.json') as Record<string, unknown>;
    expect(conform({ ...deployment, extra: 1 }, deploymentShape)).toEqual([
      '$.extra: not declared',
    ]);
    const { system: _system, ...withoutSystem } = deployment;
    expect(conform(withoutSystem, deploymentShape)).toEqual(['$.system: missing']);
    const badKind = structuredClone(deployment) as unknown as Deployment;
    (badKind.bindings[0] as { kind: string }).kind = 'cron';
    expect(conform(badKind, deploymentShape)).toEqual([
      '$.bindings[0].kind: "cron" is not one of periodic, event',
    ]);
  });

  test('the fixtures describe one rally deployment whose samples count upward', () => {
    const deployment = read('deployment.json') as Deployment;
    expect(deployment.system).toBe('rally');
    expect(deployment.components.map((c) => c.name)).toEqual([
      'rally-table',
      'rally-player-west',
      'rally-player-east',
    ]);
    for (const instance of deployment.instances) {
      const catalogue = read(`catalogue.${instance.component}.json`) as Catalogue;
      const declared = new Set(catalogue.counters.map((c) => c.name));
      const samples = read(`samples.${instance.instance_id}.json`) as Sample[];
      samples.forEach((sample, index) => {
        expect(sample.instance_id).toBe(instance.instance_id);
        expect(sample.sequence).toBe(index + 1);
        for (const counter of sample.counters) expect(declared.has(counter.name), counter.name).toBe(true);
        for (const histogram of sample.histograms) {
          expect(declared.has(histogram.name), histogram.name).toBe(true);
          expect(histogram.counts).toHaveLength(histogram.bounds.length + 1);
        }
        for (const answer of sample.recent) {
          expect(new TextEncoder().encode(answer.payload ?? '').length).toBeLessThanOrEqual(1024);
        }
      });
      const first = samples[0]!;
      const last = samples[samples.length - 1]!;
      for (const counter of first.counters) {
        const later = last.counters.find((c) => c.name === counter.name);
        expect(later?.value ?? -1).toBeGreaterThanOrEqual(counter.value);
      }
      const rose = last.counters.filter(
        (c) => c.value > (first.counters.find((f) => f.name === c.name)?.value ?? 0),
      );
      expect(rose.length, instance.instance_id).toBeGreaterThan(0);
    }
  });
});
