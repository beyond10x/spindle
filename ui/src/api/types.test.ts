// @vitest-environment node
import { describe, expect, test } from 'vitest';
import {
  catalogueShape,
  deploymentShape,
  metricFamiliesShape,
  requestLogShape,
  sampleShape,
  stateChangeShape,
  topologyShape,
} from './shapes';
import { conform } from '../lib/conform';
import { files, raw, root } from '../../dev/observe-fixtures';
import type { Catalogue, Deployment, MetricFamilies, RequestLog, Sample, Topology } from './types';

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

describe('the_ingress_answers_match_the_api', () => {
  test('a request log fixture reads as RequestLog', () => {
    const logs = files('requests');
    expect(logs.length).toBeGreaterThan(0);
    for (const file of logs) {
      expect(conform(raw(file), requestLogShape), file).toEqual([]);
      const log = raw(file) as RequestLog;
      expect(log.format, file).toBe('spindle-requests/1');
      expect(log.requests.length, file).toBeLessThanOrEqual(log.capacity);
      const { served, refused, not_granted, not_served, failed, unrouted } = log.totals;
      expect(served + refused + not_granted + not_served + failed + unrouted, file).toBe(log.recorded);
      const sequences = log.requests.map((r) => r.sequence);
      expect(sequences, file).toEqual([...sequences].sort((a, b) => b - a));
      for (const record of log.requests) {
        expect(record.reason === undefined, `${file} #${record.sequence}`).toBe(record.class === 'served');
        expect(record.route === undefined, `${file} #${record.sequence}`).toBe(record.class === 'unrouted');
      }
    }
  });

  test('a request with a class the type does not declare is refused', () => {
    const log = structuredClone(raw('requests', 'scripted.json')) as RequestLog;
    (log.requests[0] as { class: string }).class = 'lost';
    expect(conform(log, requestLogShape)).toEqual([
      '$.requests[0].class: "lost" is not one of failed, not-granted, not-served, refused, served, unrouted',
    ]);
  });

  // The class follows who answered: a declared outcome is `served` whatever status its binding
  // gives it. The fixture binds `closed` to 501 and `already-done` to 503.
  test('a declared outcome bound to a 5xx status is served', () => {
    const log = raw('requests', 'declared-5xx.json') as RequestLog;
    const fivexx = log.requests.filter((r) => r.status >= 500);
    expect(fivexx.map((r) => r.status).sort()).toEqual([501, 503]);
    for (const record of fivexx) {
      expect(record.outcome, `#${record.sequence}`).toBeDefined();
      expect(record.class, `#${record.sequence}`).toBe('served');
      expect(record.reason, `#${record.sequence}`).toBeUndefined();
    }
    expect(log.totals.not_served + log.totals.failed).toBe(0);
  });

  test('a request log or topology of another format is refused', () => {
    const log = structuredClone(raw('requests', 'scripted.json')) as Record<string, unknown>;
    expect(conform({ ...log, format: 'spindle-requests/2' }, requestLogShape)).toEqual([
      '$.format: "spindle-requests/2" is not one of spindle-requests/1',
    ]);
    const topology = structuredClone(raw('topology', 'tasks.json')) as Topology;
    expect(conform({ ...topology, format: 'spindle-topology/2' }, topologyShape)).toEqual([
      '$.format: "spindle-topology/2" is not one of spindle-topology/1',
    ]);
    (topology.edges[0]!.traffic!.statuses[0] as { status_class: string }).status_class = '6xx';
    expect(conform(topology, topologyShape)).toEqual([
      '$.edges[0].traffic.statuses[0].status_class: "6xx" is not one of 2xx, 3xx, 4xx, 5xx',
    ]);
  });

  test('the metric families fixture reads as MetricFamilies', () => {
    expect(conform(raw('metrics', 'families.json'), metricFamiliesShape)).toEqual([]);
    const { families } = raw('metrics', 'families.json') as MetricFamilies;
    const names = families.map((f) => f.name);
    expect(new Set(names).size).toBe(names.length);
    for (const name of names) expect(name).toMatch(/^spindle_[a-z_]+$/);
  });

  test('a topology fixture reads as Topology', () => {
    const graphs = files('topology');
    expect(graphs.length).toBeGreaterThan(0);
    for (const file of graphs) {
      expect(conform(raw(file), topologyShape), file).toEqual([]);
      const topology = raw(file) as Topology;
      expect(topology.format, file).toBe('spindle-topology/1');
      const ids = topology.nodes.map((n) => n.id);
      expect(new Set(ids).size, file).toBe(ids.length);
      expect(topology.nodes.filter((n) => n.kind === 'ingress'), file).toHaveLength(1);
      for (const edge of topology.edges) {
        const name = `${file} ${edge.from} -${edge.kind}-> ${edge.to}`;
        expect(ids, name).toContain(edge.from);
        expect(ids, name).toContain(edge.to);
        expect(edge.traffic === undefined, name).toBe(edge.kind === 'contains');
      }
    }
  });
});
