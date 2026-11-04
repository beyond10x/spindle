import { mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import TopologyScreen from './TopologyScreen.vue';
import deploymentJson from '../fixtures/deployment.json';
import tableSamples from '../fixtures/samples.rally-table-0.json';
import westSamples from '../fixtures/samples.rally-player-west-0.json';
import eastSamples from '../fixtures/samples.rally-player-east-0.json';
import type { Deployment, Sample } from '../api/types';

const deployment = deploymentJson as Deployment;
const samples: Record<string, Sample[]> = {
  'rally-table-0': tableSamples as Sample[],
  'rally-player-west-0': westSamples as Sample[],
  'rally-player-east-0': eastSamples as Sample[],
};

describe('topology', () => {
  test('the_topology_shows_one_card_per_component_and_one_edge_per_binding', () => {
    const wrapper = mount(TopologyScreen, { props: { deployment, samples } });

    const cards = wrapper.findAll('[data-test="component-card"]');
    expect(cards.map((card) => card.attributes('data-component'))).toEqual(
      deployment.components.map((component) => component.name),
    );

    const edges = wrapper.findAll('[data-test="binding-edge"]');
    expect(edges).toHaveLength(deployment.bindings.length);
    expect(
      edges.map((edge) => [
        edge.attributes('data-binding'),
        edge.attributes('data-from'),
        edge.attributes('data-to'),
      ]),
    ).toEqual(
      deployment.bindings.map((binding) => [
        binding.name,
        binding.from ?? 'clock',
        binding.to,
      ]),
    );
  });

  test('each card shows its instance state, target and quota use', () => {
    const wrapper = mount(TopologyScreen, { props: { deployment, samples } });
    const table = wrapper.find('[data-component="rally-table"]');
    expect(table.text()).toContain('Running');
    expect(table.text()).toContain('QemuMicrovm');
    expect(table.text()).toContain('1 vCPU');
    expect(table.text()).toMatch(/\/ 128 MiB/);
    expect(table.findAll('[data-test="sparkline"]')).toHaveLength(2);
  });

  test('an edge carries the live rate of its binding', () => {
    const wrapper = mount(TopologyScreen, { props: { deployment, samples } });
    const tick = wrapper.find('[data-binding="tick"]');
    expect(tick.text()).toContain('1.0/s');
  });
});
