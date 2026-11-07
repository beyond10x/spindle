import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import Banner from './Banner.vue';
import Panel from './Panel.vue';
import * as fixtures from '../../dev/observe-fixtures';
import type { RequestLog } from '../api/types';

const scripted = (): RequestLog => fixtures.raw('requests', 'scripted.json') as RequestLog;

/** A `fetch` answering every request with `log`, keeping each URL it was asked for. */
function recording(log: RequestLog): { fetch: typeof fetch; asked: string[] } {
  const asked: string[] = [];
  const fetcher = (input: RequestInfo | URL): Promise<Response> => {
    asked.push(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url);
    return Promise.resolve(new Response(JSON.stringify(log), { status: 200, headers: { 'content-type': 'application/json' } }));
  };
  return { fetch: fetcher as typeof fetch, asked };
}

describe('panel', () => {
  test('the banner reads its counts from the totals', () => {
    const log = scripted();
    const totals = { ...log.totals, served: 11, not_served: 3, refused: 5, not_granted: 2, unrouted: 7, failed: 13 };
    const wrapper = mount(Banner, { props: { totals } });
    const banner = wrapper.get('[data-test="banner"]');
    const text = banner.text();
    expect(text).not.toContain('\n');
    const count = (name: string) => banner.get(`[data-test="count-${name}"]`).text();
    expect(count('served')).toBe('11');
    expect(count('not-served')).toBe('3');
    expect(count('refused')).toBe('5');
    expect(count('not-granted')).toBe('2');
    expect(count('unrouted')).toBe('7');
    expect(text).toMatch(/served 11.*not served 3.*refused 5.*not granted 2.*unrouted 7/);
  });

  test('the not-served toggle asks for class=not-served', async () => {
    const { fetch, asked } = recording(scripted());
    const wrapper = mount(Panel, { props: { fetch, every: 0 } });
    await flushPromises();
    expect(asked).toEqual(['./requests']);
    expect(wrapper.findAll('[data-test="request"]')).toHaveLength(scripted().requests.length);
    await wrapper.get('[data-test="not-served-only"]').setValue(true);
    await flushPromises();
    expect(asked.at(-1)).toBe('./requests?class=not-served');
    await wrapper.get('[data-test="not-served-only"]').setValue(false);
    await flushPromises();
    expect(asked.at(-1)).toBe('./requests');
  });

  test('the panel only reads reserved routes relative to itself', async () => {
    const { fetch, asked } = recording(scripted());
    const wrapper = mount(Panel, { props: { fetch, every: 0 } });
    await flushPromises();
    await wrapper.get('[data-test="not-served-only"]').setValue(true);
    await flushPromises();
    await wrapper.get('[data-test="refresh"]').trigger('click');
    await flushPromises();
    await wrapper.get('[data-test="not-served-only"]').setValue(false);
    await flushPromises();
    expect(asked.length).toBeGreaterThan(0);
    for (const url of asked) {
      expect(url).toMatch(/^\.\/(requests|metrics|topology)(\?[^/]*)?$/);
    }
  });
});
