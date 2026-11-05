import { mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import InstanceScreen from './InstanceScreen.vue';
import * as fixtures from '../../dev/observe-fixtures';

const instance = fixtures.deployment().instances.find((i) => i.instance_id === 'rally-table-0')!;
const catalogue = fixtures.catalogue(instance.component)!;
const samples = fixtures.samples(instance.instance_id);

describe('instance', () => {
  test('the_instance_screen_shows_one_instances_counters_and_quota', () => {
    const wrapper = mount(InstanceScreen, { props: { instance, catalogue, samples } });
    const last = samples[samples.length - 1]!;
    const before = samples[samples.length - 2]!;
    const span = (Date.parse(last.at) - Date.parse(before.at)) / 1000;

    const commands = catalogue.counters.filter((c) => c.kind === 'counter' && c.name.startsWith('command.'));
    expect(commands.length).toBeGreaterThan(0);
    for (const declared of commands) {
      const row = wrapper.find(`[data-test="commands"] [data-counter="${declared.name}"]`);
      expect(row.exists(), declared.name).toBe(true);
      const now = last.counters.find((c) => c.name === declared.name)?.value;
      const then = before.counters.find((c) => c.name === declared.name)?.value;
      const cells = row.findAll('td').map((cell) => cell.text());
      if (now === undefined) {
        expect(cells[2]).toBe('never reported');
        continue;
      }
      expect(cells[2], declared.name).toBe(String(now));
      expect(cells[3], declared.name).toBe(`${((now - (then ?? now)) / span).toFixed(1)}/s`);
    }

    const cpu = (last.resources.cpu_seconds - before.resources.cpu_seconds) / (span * instance.requested.vcpus);
    expect(wrapper.find('[data-test="cpu-share"]').text()).toBe(
      `${(cpu * 100).toFixed(0)} % of ${instance.requested.vcpus}`,
    );
    const memory = last.resources.memory_share!;
    expect(wrapper.find('[data-test="memory-share"]').text()).toBe(
      `${(last.resources.memory_bytes / 1048576).toFixed(0)} / ${instance.requested.memory_mib} MiB · ${(memory * 100).toFixed(0)} %`,
    );
  });
});
