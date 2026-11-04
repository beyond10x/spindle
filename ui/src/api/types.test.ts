// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { catalogueShape, deploymentShape, sampleShape, stateChangeShape } from './shapes';
import { conform } from '../lib/conform';
import { files, raw, root } from '../../dev/observe-fixtures';
import type { Catalogue, Deployment, Sample } from './types';

const instanceDirs = () => {
  const deployment = raw('deployment.json') as Deployment;
  return deployment.instances.map((i) => i.instance_id);
};

describe('the_ui_types_match_the_api', () => {
  test('the contract fixtures are where the UI reads them', () => {
    expect(root.replaceAll('\\', '/')).toMatch(/\/ess\/fixtures\/observe$/);
  });

  test('the deployment fixture has exactly the fields of Deployment', () => {
    expect(conform(raw('deployment.json'), deploymentShape)).toEqual([]);
  });

  test('every catalogue fixture has exactly the fields of Catalogue', () => {
    const catalogues = files('catalogue');
    expect(catalogues).toHaveLength(3);
    for (const file of catalogues) expect(conform(raw(file), catalogueShape), file).toEqual([]);
  });

  test('every sample fixture has exactly the fields of Sample', () => {
    for (const instance of instanceDirs()) {
      const samples = files(`samples/${instance}`);
      expect(samples, instance).toHaveLength(30);
      for (const file of samples) expect(conform(raw(file), sampleShape), file).toEqual([]);
    }
  });

  test('every state change fixture has exactly the fields of StateChange', () => {
    const changes = files('state');
    expect(changes.length).toBeGreaterThan(0);
    for (const file of changes) expect(conform(raw(file), stateChangeShape), file).toEqual([]);
  });

  test('a field the type does not declare is refused, and so is a missing or out-of-range one', () => {
    const deployment = raw('deployment.json') as Record<string, unknown>;
    expect(conform({ ...deployment, extra: 1 }, deploymentShape)).toEqual([
      '$.extra: not declared',
    ]);
    const { system: _system, ...withoutSystem } = deployment;
    expect(conform(withoutSystem, deploymentShape)).toEqual(['$.system: missing']);
    const changed = structuredClone(deployment) as unknown as Deployment;
    (changed.bindings[0] as { kind: string }).kind = 'cron';
    (changed.instances[0] as { target_kind: string }).target_kind = 'Mainframe';
    (changed.instances[0] as { state: string }).state = 'Sleeping';
    expect(conform(changed, deploymentShape)).toEqual([
      '$.bindings[0].kind: "cron" is not one of event, periodic',
      '$.instances[0].target_kind: "Mainframe" is not one of Browser, Compose, Docker, Firecracker, Kubernetes, Process, QemuMicrovm',
      '$.instances[0].state: "Sleeping" is not one of Exited, Placed, Planned, Refused, Running',
    ]);
  });

  test('the fixtures describe one rally deployment whose samples count upward', () => {
    const deployment = raw('deployment.json') as Deployment;
    expect(deployment.system).toBe('rally');
    expect(deployment.components.map((c) => c.name).sort()).toEqual([
      'rally-player-east',
      'rally-player-west',
      'rally-table',
    ]);
    for (const instance of deployment.instances) {
      const catalogue = raw('catalogue', `${instance.component}.json`) as Catalogue;
      const declared = new Map(catalogue.counters.map((c) => [c.name, c.kind]));
      const samples = files(`samples/${instance.instance_id}`).map((file) => raw(file) as Sample);
      samples.forEach((sample, index) => {
        expect(sample.instance_id).toBe(instance.instance_id);
        if (index > 0) expect(sample.sequence).toBe(samples[index - 1]!.sequence + 1);
        for (const counter of sample.counters) expect(declared.get(counter.name), counter.name).toBe('counter');
        for (const histogram of sample.histograms) {
          expect(declared.get(histogram.name), histogram.name).toBe('histogram');
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
