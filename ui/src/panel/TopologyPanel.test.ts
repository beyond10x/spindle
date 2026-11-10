import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import Panel from './Panel.vue';
import TopologyPanel from './TopologyPanel.vue';
import * as fixtures from '../../dev/observe-fixtures';
import type { Topology } from '../api/types';

const tasks = (): Topology => fixtures.raw('topology', 'tasks.json') as Topology;

/** A `fetch` answering `./topology` with `body` and `./requests` with the scripted log, keeping each URL it was asked for. */
function recording(body: unknown): { fetch: typeof fetch; asked: string[] } {
  const asked: string[] = [];
  const fetcher = (input: RequestInfo | URL): Promise<Response> => {
    asked.push(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url);
    const answer = asked.at(-1)!.startsWith('./requests') ? fixtures.raw('requests', 'scripted.json') : body;
    return Promise.resolve(new Response(JSON.stringify(answer), { status: 200, headers: { 'content-type': 'application/json' } }));
  };
  return { fetch: fetcher as typeof fetch, asked };
}

const number = (value: string | undefined): number => Number(value ?? Number.NaN);

describe('topology screen', () => {
  test('the topology screen draws the ingress first and one edge per routes edge', async () => {
    const topology = tasks();
    const { fetch, asked } = recording(topology);
    const wrapper = mount(TopologyPanel, { props: { fetch, every: 0 } });
    await flushPromises();
    expect(asked).toEqual(['./topology']);
    const svg = wrapper.get('svg[data-test="topology"]');
    const nodes = svg.findAll('[data-test="node"]');
    expect(nodes).toHaveLength(topology.nodes.length);
    expect(nodes[0]!.attributes('data-id')).toBe('ingress');
    const top = number(nodes[0]!.attributes('data-y'));
    for (const node of nodes.slice(1)) {
      expect(number(node.attributes('data-y'))).toBeGreaterThan(top);
    }
    const routes = topology.edges.filter((edge) => edge.kind === 'routes');
    expect(routes.length).toBeGreaterThan(0);
    const drawn = svg.findAll('[data-test="edge"][data-kind="routes"]');
    expect(drawn).toHaveLength(routes.length);
    expect(drawn.map((edge) => edge.attributes('data-to'))).toEqual(routes.map((edge) => edge.to));
    expect(svg.findAll('[data-test="edge"]')).toHaveLength(topology.edges.length);
    for (const edge of drawn) {
      expect(edge.get('path').attributes('d')).toMatch(/^M [\d.]+ [\d.]+ C /);
    }
  });

  test("the topology screen shows each edge's status mix and latest calls", async () => {
    const topology = tasks();
    const { fetch } = recording(topology);
    const wrapper = mount(TopologyPanel, { props: { fetch, every: 0 } });
    await flushPromises();
    const routes = topology.edges.filter((edge) => edge.kind === 'routes');
    for (const edge of routes) {
      const drawn = wrapper.get(`[data-test="edge"][data-kind="routes"][data-to="${edge.to}"]`);
      const traffic = edge.traffic!;
      expect(drawn.get('[data-test="count"]').text()).toBe(String(traffic.count));
      const mix = drawn.get('[data-test="statuses"]').text();
      for (const status of traffic.statuses) {
        expect(mix).toContain(`${status.status_class} ${status.count}`);
      }
      const latest = drawn.get('[data-test="latest"]').text();
      expect(latest).toBe(traffic.latest.map((sequence) => `#${sequence}`).join(' '));
    }
    const open = routes.find((edge) => edge.to === 'command:tasks.board.OpenTask')!;
    const drawn = wrapper.get(`[data-test="edge"][data-kind="routes"][data-to="${open.to}"]`);
    expect(drawn.get('[data-test="statuses"]').text()).toBe('2xx 1 · 4xx 1');
    expect(drawn.get('[data-test="latest"]').text()).toBe('#4 #0');
  });

  test('the panel switches to the topology screen and reads ./topology beside itself', async () => {
    const { fetch, asked } = recording(tasks());
    const wrapper = mount(Panel, { props: { fetch, every: 0 } });
    await flushPromises();
    expect(asked).toEqual(['./requests']);
    await wrapper.get('[data-test="screen-topology"]').trigger('click');
    await flushPromises();
    expect(asked.at(-1)).toBe('./topology');
    expect(wrapper.find('svg[data-test="topology"]').exists()).toBe(true);
    await wrapper.get('[data-test="screen-requests"]').trigger('click');
    await flushPromises();
    expect(wrapper.find('svg[data-test="topology"]').exists()).toBe(false);
  });
});
