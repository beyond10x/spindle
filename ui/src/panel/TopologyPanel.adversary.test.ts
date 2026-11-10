import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import TopologyPanel from './TopologyPanel.vue';
import { draw } from './topology';
import type { Topology, TopologyEdge, TopologyNode } from '../api/types';

function answering(body: unknown): typeof fetch {
  return (() =>
    Promise.resolve(
      new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } }),
    )) as typeof fetch;
}

const empty = (): Topology =>
  ({ format: 'spindle-topology/1', at: '2026-11-07T09:00:01.000Z', specifications: [], nodes: [], edges: [] }) as Topology;

function huge(commands: number): Topology {
  const nodes: TopologyNode[] = [
    { id: 'ingress', kind: 'ingress', name: 'ingress' },
    { id: 'big', kind: 'specification', name: 'big', specification: 'big' },
    { id: 'domain:big.d', kind: 'domain', name: 'big.d', specification: 'big' },
  ];
  const edges: TopologyEdge[] = [{ from: 'big', to: 'domain:big.d', kind: 'contains' }];
  for (let index = 0; index < commands; index++) {
    const id = `command:big.d.C${index}`;
    nodes.push({ id, kind: 'command', name: `big.d.C${index}`, specification: 'big' });
    edges.push({
      from: 'ingress',
      to: id,
      kind: 'routes',
      traffic: { count: index, statuses: [], sources: [], latest: [] },
    });
  }
  return { format: 'spindle-topology/1', at: '2026-11-07T09:00:01.000Z', specifications: ['big'], nodes, edges } as Topology;
}

describe('topology screen, adversary', () => {
  test('an empty graph draws an SVG of no negative size', async () => {
    const drawing = draw(empty());
    expect(drawing.width).toBeGreaterThanOrEqual(0);
    expect(drawing.height).toBeGreaterThanOrEqual(0);
    const wrapper = mount(TopologyPanel, { props: { fetch: answering(empty()), every: 0 } });
    await flushPromises();
    const svg = wrapper.find('svg[data-test="topology"]');
    if (svg.exists()) {
      expect(Number(svg.attributes('height'))).toBeGreaterThanOrEqual(0);
      expect(svg.attributes('viewBox')).not.toMatch(/-/);
    }
  });

  test('a graph of 2000 commands draws every node and edge on finite coordinates', async () => {
    const topology = huge(2000);
    const drawing = draw(topology);
    expect(drawing.nodes).toHaveLength(topology.nodes.length);
    expect(drawing.edges).toHaveLength(topology.edges.length);
    expect(Number.isFinite(drawing.width)).toBe(true);
    const wrapper = mount(TopologyPanel, { props: { fetch: answering(topology), every: 0 } });
    await flushPromises();
    expect(wrapper.findAll('[data-test="node"]')).toHaveLength(topology.nodes.length);
    expect(wrapper.findAll('[data-test="edge"][data-kind="routes"]')).toHaveLength(2000);
  });

  test("a specification called `ingress` keeps the ingress's edges on the ingress", () => {
    const topology: Topology = {
      format: 'spindle-topology/1',
      at: '2026-11-07T09:00:01.000Z',
      specifications: ['ingress'],
      nodes: [
        { id: 'ingress', kind: 'ingress', name: 'ingress' },
        { id: 'ingress', kind: 'specification', name: 'ingress', specification: 'ingress' },
        { id: 'domain:ingress.d', kind: 'domain', name: 'ingress.d', specification: 'ingress' },
        { id: 'command:ingress.d.C', kind: 'command', name: 'ingress.d.C', specification: 'ingress' },
      ],
      edges: [{ from: 'ingress', to: 'command:ingress.d.C', kind: 'routes', traffic: { count: 1, statuses: [], sources: [], latest: [] } }],
    } as Topology;
    const drawing = draw(topology);
    const ingress = drawing.nodes.find((placed) => placed.node.kind === 'ingress')!;
    const routes = drawing.edges[0]!;
    expect(routes.d.startsWith(`M ${ingress.x + 95} ${ingress.y + 26} `)).toBe(true);
  });
});
