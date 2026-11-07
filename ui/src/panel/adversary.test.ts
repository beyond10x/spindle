// Adversarial cases for the observe panel: the inlining plugin against the HTML rules for the
// text of a script element, and the panel against answers arriving out of order.

import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import type { Plugin } from 'vite';
import Panel from './Panel.vue';
import { inline } from '../../dev/inline';
import * as fixtures from '../../dev/observe-fixtures';
import type { RequestLog } from '../api/types';

const scripted = (): RequestLog => fixtures.raw('requests', 'scripted.json') as RequestLog;

/** `bundle` after the plugin's `generateBundle` ran over it. */
function inlined(page: string, code: string): string {
  const bundle: Record<string, unknown> = {
    'panel.html': { type: 'asset', fileName: 'panel.html', source: page },
    'assets/panel.js': { type: 'chunk', fileName: 'assets/panel.js', code },
  };
  const hook = (inline() as Plugin & { generateBundle: (...args: unknown[]) => void }).generateBundle;
  hook.call({}, {}, bundle, false);
  return String((bundle['panel.html'] as { source: unknown }).source);
}

const isEndOfName = (c: string | undefined): boolean => c === undefined || /[\t\n\f\r />]/.test(c);
const startsTag = (text: string, at: number, tag: string): boolean =>
  text.slice(at, at + tag.length).toLowerCase() === tag && isEndOfName(text[at + tag.length]);

/**
 * Where the script element whose text starts at `from` ends, as the HTML tokenizer's script data
 * states find its end tag (script data, escaped after `<!--`, double escaped after `<script`
 * inside that), or -1 where it never ends.
 */
function scriptEnd(html: string, from: number): number {
  let state: 'data' | 'escaped' | 'double' = 'data';
  for (let at = from; at < html.length; at++) {
    if (state !== 'data' && html.startsWith('-->', at)) {
      state = 'data';
      at += 2;
      continue;
    }
    if (html[at] !== '<') continue;
    if (state === 'data' && html.startsWith('<!--', at)) {
      state = 'escaped';
      at += 3;
      continue;
    }
    if (startsTag(html, at, '</script')) {
      if (state === 'double') state = 'escaped';
      else return at;
      continue;
    }
    if (state === 'escaped' && startsTag(html, at, '<script')) state = 'double';
  }
  return -1;
}

const PAGE =
  '<!doctype html><html><head><script type="module" crossorigin src="./assets/panel.js"></script></head>' +
  '<body><div id="panel"></div></body></html>';

describe('panel, adversarial', () => {
  test('an inlined script holding <!--<script> still ends where the plugin closes it', () => {
    const code = 'const marker = "<!--<script>"; console.log(marker);';
    const html = inlined(PAGE, code);
    const open = '<script type="module">';
    const start = html.indexOf(open);
    expect(start).toBeGreaterThanOrEqual(0);
    const end = scriptEnd(html, start + open.length);
    expect(end, 'the script element never ends: the rest of the page is script text').toBeGreaterThan(0);
    expect(html.slice(end)).toContain('<div id="panel">');
  });

  test('a stale answer does not overwrite the not-served list asked for after it', async () => {
    const full = scripted();
    const notServed: RequestLog = {
      ...full,
      requests: full.requests.filter((request) => request.class === 'not-served'),
    };
    expect(notServed.requests.length).toBeLessThan(full.requests.length);
    const answer = (log: RequestLog) =>
      new Response(JSON.stringify(log), { status: 200, headers: { 'content-type': 'application/json' } });
    let releaseFirst: (response: Response) => void = () => {};
    const asked: string[] = [];
    const fetcher = ((input: RequestInfo | URL): Promise<Response> => {
      const url = String(input);
      asked.push(url);
      if (asked.length === 1) return new Promise((resolve) => (releaseFirst = resolve));
      return Promise.resolve(answer(url.includes('class=not-served') ? notServed : full));
    }) as typeof fetch;
    const wrapper = mount(Panel, { props: { fetch: fetcher, every: 0 } });
    await flushPromises();
    await wrapper.get('[data-test="not-served-only"]').setValue(true);
    await flushPromises();
    expect(wrapper.findAll('[data-test="request"]')).toHaveLength(notServed.requests.length);
    // The unfiltered read made on mount answers only now, after the toggle's.
    releaseFirst(answer(full));
    await flushPromises();
    expect(asked).toEqual(['./requests', './requests?class=not-served']);
    expect(
      wrapper.findAll('[data-test="request"]'),
      'the checkbox says "not served only" and the list shows every request',
    ).toHaveLength(notServed.requests.length);
  });
});
