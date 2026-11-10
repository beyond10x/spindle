// The layout contract for one page (L2, docs variant): opened at one viewport in one theme, read at
// its top and at its end (scrolled to the bottom), with hard gates 1, 2 and 5 read off the page.

import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import AxeBuilder from '@axe-core/playwright';
import type { BrowserContext, Page } from '@playwright/test';
import { fullyVisible, overflow, reading, scrollBoxes, signature } from './probe.ts';
import type { Overflow, Reading, Visibility } from './probe.ts';
import type { Adapter, SitePage, Theme, Viewport } from './types.ts';

export const AXE_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

export interface AxeViolation {
  id: string;
  impact: string;
  help: string;
  nodes: string[];
}

export interface StateCheck {
  state: 'top' | 'end';
  overflow: Overflow[];
  hidden: Visibility[];
}

export interface PageCheck {
  page: string;
  path: string;
  viewport: string;
  theme: string;
  status: number;
  states: StateCheck[];
  axe: AxeViolation[] | null;
  reading: Reading;
  scroll_boxes: number;
  /** Layout shift from load to the page's end: the sum of `layout-shift` entries without recent input. */
  layout_shift: number | null;
  shot: string | null;
}

/** Sums every layout shift without recent input from the first paint on, into `window.__uxShift`. Self-contained: sent to the page. */
export function shiftObserver(): void {
  const w = window as unknown as { __uxShift: number };
  w.__uxShift = 0;
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries() as unknown as { value: number; hadRecentInput: boolean }[]) if (!e.hadRecentInput) w.__uxShift += e.value;
    }).observe({ type: 'layout-shift', buffered: true });
  } catch {
    w.__uxShift = Number.NaN;
  }
}

export const shiftSoFar = (page: Page): Promise<number> => page.evaluate(() => (window as unknown as { __uxShift?: number }).__uxShift ?? Number.NaN);

/** Every request off the served site is answered by a stub page naming the URL: hermetic. */
export async function hermetic(context: BrowserContext, origin: string): Promise<void> {
  await context.route('**/*', (route) => {
    const url = route.request().url();
    if (url.startsWith(origin) || url.startsWith('data:')) return route.continue();
    return route.fulfill({
      status: 200,
      contentType: 'text/html; charset=utf-8',
      body: `<!doctype html><title>Off the site</title><main><h1>Off the site</h1><p data-ux-external>${url.replace(/</g, '&lt;')}</p></main>`,
    });
  });
}

export async function prepare(page: Page, viewport: Viewport, theme: Theme): Promise<void> {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await page.emulateMedia({ colorScheme: theme.colorScheme, reducedMotion: 'reduce' });
}

/** Waits until the page is interactive and three reads of where it stands, 40 ms apart, agree (at most about 6 s). */
export async function settle(page: Page, notReady: string): Promise<void> {
  await page.waitForLoadState('load');
  await page.waitForFunction((sel) => !document.querySelector(sel), notReady, { timeout: 10_000 }).catch(() => undefined);
  let last = '';
  let same = 0;
  for (let i = 0; i < 150 && same < 2; i += 1) {
    await page.waitForTimeout(40);
    const now = await page.evaluate(signature);
    same = now === last ? same + 1 : 0;
    last = now;
  }
}

/** The theme the page shows must be the one asked for: a theme check that checks the wrong theme proves nothing. */
export async function assertTheme(page: Page, theme: Theme): Promise<void> {
  const shown = await page.evaluate(() => document.documentElement.dataset.theme ?? '');
  if (shown !== theme.colorScheme) throw new Error(`the page shows theme "${shown}", not "${theme.colorScheme}"`);
}

/** Serious and critical axe-core violations. */
export async function axe(page: Page): Promise<AxeViolation[]> {
  const result = await new AxeBuilder({ page }).withTags(AXE_TAGS).analyze();
  return result.violations
    .filter((v) => v.impact === 'serious' || v.impact === 'critical')
    .map((v) => ({ id: v.id, impact: v.impact ?? '', help: v.help, nodes: v.nodes.map((n) => n.target.join(' ')) }));
}

export const axeNodes = (violations: readonly AxeViolation[] | null): number => (violations ?? []).reduce((n, v) => n + v.nodes.length, 0);

export interface CheckOptions {
  axe?: boolean;
  shots?: string;
}

export async function checkPage(page: Page, adapter: Adapter, site: SitePage, viewport: Viewport, theme: Theme, options: CheckOptions = {}): Promise<PageCheck> {
  await prepare(page, viewport, theme);
  await page.addInitScript(shiftObserver);
  const response = await page.goto(`${adapter.base}${site.path}`);
  await settle(page, adapter.notReady);
  await assertTheme(page, theme);
  const required = adapter.alwaysVisible(viewport);
  const applicable = await page.evaluate(
    (list) => list.filter((r) => !r.onlyWhere || document.querySelector(r.onlyWhere)).map(({ name, selector }) => ({ name, selector })),
    required,
  );
  const states: StateCheck[] = [];
  const read = async (state: 'top' | 'end') => {
    states.push({ state, overflow: await page.evaluate(overflow), hidden: (await page.evaluate(fullyVisible, applicable)).filter((v) => !v.ok) });
  };
  await read('top');
  let shot: string | null = null;
  if (options.shots) {
    mkdirSync(options.shots, { recursive: true });
    shot = join(options.shots, `${site.name}--${viewport.name}--${theme.name}.png`);
    await page.screenshot({ path: shot });
  }
  const found = options.axe ? await axe(page) : null;
  const text = await page.evaluate(reading, adapter.article);
  const boxes = await page.evaluate(scrollBoxes);
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await settle(page, adapter.notReady);
  await read('end');
  const shift = await shiftSoFar(page);
  return {
    page: site.name,
    path: site.path,
    viewport: viewport.name,
    theme: theme.name,
    status: response?.status() ?? 0,
    states,
    axe: found,
    reading: text,
    scroll_boxes: boxes,
    layout_shift: Number.isFinite(shift) ? shift : null,
    shot,
  };
}
