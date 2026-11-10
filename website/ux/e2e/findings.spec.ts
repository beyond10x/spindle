// Regression assets of the findings (SKILL.md § 5): one assertion per finding, named with its id.
// Each was written red against the site of R-00 and turned green by its change.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { adapter, origin } from '../adapter.ts';
import { play } from '../core/bot.ts';
import { axe, checkPage, hermetic, prepare, settle } from '../core/page.ts';
import { fullyVisible } from '../core/probe.ts';

test.beforeEach(async ({ context }) => hermetic(context, origin));

const phone = adapter.gating.find((v) => v.width < 997)!;
const desktop = adapter.gating.find((v) => v.width >= 997)!;
const page = (name: string) => adapter.pages.find((p) => p.name === name)!;
const task = (id: string) => adapter.tasks.find((t) => t.id === id)!;

for (const theme of adapter.themes) {
  test(`UXF-002 the theme toggle is fully in view on a phone, ${theme.name}`, async ({ page: p }) => {
    for (const name of ['landing', 'getting-started']) {
      await prepare(p, phone, theme);
      await p.goto(`${adapter.base}${page(name).path}`);
      await settle(p, adapter.notReady);
      const seen = await p.evaluate(fullyVisible, [{ name: 'theme toggle', selector: 'nav.navbar [class*="colorModeToggle"] button' }]);
      expect(seen.map((v) => `${name}: ${v.why}`)).toEqual([`${name}: in view`]);
    }
  });

  test(`UXF-003 links in running text are not told apart by colour alone, ${theme.name}`, async ({ page: p }) => {
    const run = await checkPage(p, adapter, page('getting-started'), desktop, theme, {});
    expect(run.status).toBe(200);
    expect((await axe(p)).filter((v) => v.id === 'link-in-text-block')).toEqual([]);
    const decoration = await p.evaluate(() => getComputedStyle(document.querySelector('article .markdown p a[href$="rally"]')!).textDecorationLine);
    expect(decoration).toBe('underline');
  });
}

for (const viewport of adapter.gating) {
  test(`UXF-004 a reader reaches serving over HTTP from the landing page, ${viewport.name}`, async ({ page: p }) => {
    const run = await play(p, adapter, task('http'), viewport, adapter.themes[0]!, 'pointer');
    expect(run.reached, `${run.stuck ?? ''}: ${run.path.join(' → ')}`).toBe(true);
    expect(run.actions).toBeLessThanOrEqual(viewport === desktop ? 3 : 6);
  });
}

test(`UXF-005 a reader reaches what run --observe serves from the rally page, ${desktop.name}`, async ({ page: p }) => {
  const run = await play(p, adapter, task('observe'), desktop, adapter.themes[0]!, 'pointer');
  expect(run.reached, `${run.stuck ?? ''}: ${run.path.join(' → ')}`).toBe(true);
  expect(run.detours).toBe(0);
});

test(`UXF-005 on a phone the menu leads straight to the run section, ${phone.name}`, async ({ page: p }) => {
  // Since R-01 the reader searches before it opens the menu, so the menu is opened here directly.
  await prepare(p, phone, adapter.themes[0]!);
  await p.goto(`${adapter.base}${task('observe').start}`);
  await settle(p, adapter.notReady);
  await p.getByRole('button', { name: 'Toggle navigation bar' }).click();
  const link = p.locator('.navbar-sidebar a', { hasText: /^run --observe$/ });
  await expect(link).toBeInViewport({ ratio: 1 });
  expect(new URL((await link.getAttribute('href'))!, adapter.base).pathname).toBe(new URL(`${adapter.base}docs/reference/cli`).pathname);
});

// R-01 assets: written red against the R-01 site as measured (`UX_BUILD=.out/R-01-build`) and
// turned green by R-01's change set.

for (const viewport of adapter.gating) {
  test(`UXF-001 the navbar's search field is in view and finds a section offline, ${viewport.name}`, async ({ page: p, context }) => {
    const off: string[] = [];
    context.on('request', (r) => {
      if (!r.url().startsWith(origin) && !r.url().startsWith('data:')) off.push(r.url());
    });
    await prepare(p, viewport, adapter.themes[0]!);
    await p.goto(adapter.base);
    await settle(p, adapter.notReady);
    const field = p.getByRole('textbox', { name: 'Search' });
    await expect(field).toBeInViewport({ ratio: 1 });
    await field.click();
    await p.keyboard.type('HTTP call');
    const result = p.getByRole('option').filter({ hasText: 'An external cause bound to an HTTP call' }).first();
    await expect(result).toBeVisible({ timeout: 5000 });
    await result.click();
    await expect(p).toHaveURL(/docs\/concepts\/environment-and-admission/);
    await expect(p.getByText('An environment may declare HTTP calls')).toBeInViewport();
    expect(off).toEqual([]);
  });
}

test('UXF-007 Getting started states the release state CHANGELOG.md records', async ({ page: p }) => {
  const changelog = readFileSync(join(import.meta.dirname, '..', '..', '..', 'CHANGELOG.md'), 'utf8');
  const latest = /^## \[(\d+\.\d+\.\d+)\]/m.exec(changelog)?.[1];
  await p.goto(`${adapter.base}docs/getting-started`);
  await settle(p, adapter.notReady);
  const text = await p.locator(adapter.article).innerText();
  if (latest) {
    expect(text).not.toMatch(/no release yet/i);
    await expect(p.locator(`${adapter.article} a[href="https://github.com/beyond10x/spindle/releases"]`)).toHaveCount(1);
  }
});

test('UXF-018 a version Getting started shows is the latest release CHANGELOG.md records', async ({ page: p }) => {
  const changelog = readFileSync(join(import.meta.dirname, '..', '..', '..', 'CHANGELOG.md'), 'utf8');
  const latest = /^## \[(\d+\.\d+\.\d+)\]/m.exec(changelog)?.[1];
  await p.goto(`${adapter.base}docs/getting-started`);
  await settle(p, adapter.notReady);
  const text = await p.locator(adapter.article).innerText();
  const shown = [...text.matchAll(/b10x-spindle (\d+\.\d+\.\d+)/g)].map((m) => m[1]);
  expect(shown.length).toBeGreaterThan(0);
  expect(shown.filter((v) => v !== latest)).toEqual([]);
});

test('UXF-011 key data (table cells, code blocks, status chips and strips) is set at 14 px or more', async ({ page: p }) => {
  const small: string[] = [];
  for (const name of ['landing', 'status', 'reference-cli', 'getting-started', 'concepts-capabilities']) {
    for (const viewport of adapter.gating) {
      await prepare(p, viewport, adapter.themes[0]!);
      await p.goto(`${adapter.base}${page(name).path}`);
      await settle(p, adapter.notReady);
      const found = await p.evaluate(() => {
        const out: string[] = [];
        const keys = document.querySelectorAll('.theme-doc-markdown :is(td, th), .theme-code-block pre code, .b10x-chip, .b10x-status, .b10x-status-strip__detail');
        for (const key of Array.from(keys)) {
          const walker = document.createTreeWalker(key, NodeFilter.SHOW_TEXT);
          for (let t = walker.nextNode(); t; t = walker.nextNode()) {
            const el = t.parentElement;
            if (!el || !(t.nodeValue ?? '').trim() || el.getBoundingClientRect().width === 0) continue;
            const px = parseFloat(getComputedStyle(el).fontSize);
            if (px < 13.95) out.push(`${px}px "${(t.nodeValue ?? '').trim().slice(0, 24)}"`);
          }
        }
        return out;
      });
      small.push(...[...new Set(found)].slice(0, 3).map((f) => `${name} ${viewport.name}: ${f}`));
    }
  }
  expect(small).toEqual([]);
});

for (const viewport of adapter.gating) {
  test(`UXF-013 the "run --observe" link lands on the --observe row, ${viewport.name}`, async ({ page: p }) => {
    await prepare(p, viewport, adapter.themes[0]!);
    await p.goto(`${adapter.base}docs/concepts/observe`);
    await settle(p, adapter.notReady);
    if (viewport.width < 997) await p.getByRole('button', { name: 'Toggle navigation bar' }).click();
    await p.locator('nav a', { hasText: /^run --observe$/ }).filter({ visible: true }).first().click();
    await settle(p, adapter.notReady);
    const row = p.locator('tr', { has: p.locator('td:first-child > code', { hasText: /^--observe$/ }) });
    // The option's name, not its whole cell: a row with a long meaning is taller than a screen.
    await expect(row.locator('td:first-child > code')).toBeInViewport({ ratio: 1 });
  });
}

test('UXF-016 the concept kickers count the concept pages, without gaps', async ({ page: p }) => {
  const concepts = adapter.pages.filter((s) => s.path.startsWith('docs/concepts/'));
  const kickers: string[] = [];
  for (const site of concepts) {
    await p.goto(`${adapter.base}${site.path}`);
    await settle(p, adapter.notReady);
    kickers.push((await p.locator('.b10x-doc-header .b10x-kicker').first().innerText()).replace(/\s+/g, ' ').trim());
  }
  const counts = kickers.map((k) => /(\d+) of (\d+)/i.exec(k)).map((m): [number, number] => (m ? [Number(m[1]), Number(m[2])] : [0, 0]));
  expect(counts.map(([, total]) => total)).toEqual(concepts.map(() => concepts.length));
  expect(counts.map(([index]) => index).sort((a, b) => a - b)).toEqual(concepts.map((_, i) => i + 1));
});

// R-02 assets: written red against the R-02 site as measured (`UX_BUILD=.out/R-02-build`) and
// turned green by R-02's change set.

for (const theme of adapter.themes) {
  test(`UXF-009 on a phone every table cell is inside the screen without scrolling the table, ${theme.name}`, async ({ page: p }) => {
    const cut: string[] = [];
    for (const site of adapter.pages) {
      await prepare(p, phone, theme);
      await p.goto(`${adapter.base}${site.path}`);
      await settle(p, adapter.notReady);
      const found = await p.evaluate((article) => {
        const out: string[] = [];
        for (const cell of Array.from(document.querySelectorAll(`${article} table td`))) {
          const r = cell.getBoundingClientRect();
          if (r.width === 0 || r.height === 0 || !(cell.textContent ?? "").trim()) continue;
          if (r.left < -1 || r.right > window.innerWidth + 1) out.push(`"${(cell.textContent ?? "").trim().slice(0, 24)}" at ${Math.round(r.left)}..${Math.round(r.right)}`);
        }
        return out;
      }, adapter.article);
      if (found.length) cut.push(`${site.name}: ${found.length} cells, first ${found[0]}`);
    }
    expect(cut).toEqual([]);
  });

  test(`UXF-019 at desktop width every docs table fits its column, ${theme.name}`, async ({ page: p }) => {
    const wide: string[] = [];
    for (const site of adapter.pages) {
      await prepare(p, desktop, theme);
      await p.goto(`${adapter.base}${site.path}`);
      await settle(p, adapter.notReady);
      const found = await p.evaluate((article) =>
        Array.from(document.querySelectorAll(`${article} table`)).flatMap((t) => {
          const box = (t.closest('.b10x-table-wrap') ?? t) as HTMLElement;
          return box.scrollWidth > box.clientWidth + 1 ? [`table after "${(t.closest('.b10x-table-wrap') ?? t).previousElementSibling?.textContent?.trim().slice(0, 24)}" ${box.scrollWidth} px in ${box.clientWidth}`] : [];
        }), adapter.article);
      wide.push(...found.map((f) => `${site.name}: ${f}`));
    }
    expect(wide).toEqual([]);
  });
}

test('UXF-012 no page kicker counts a set it does not name', async ({ page: p }) => {
  const bare: string[] = [];
  for (const site of adapter.pages.filter((s) => s.path.startsWith('docs'))) {
    await p.goto(`${adapter.base}${site.path}`);
    await settle(p, adapter.notReady);
    const kicker = p.locator('.b10x-doc-header .b10x-kicker');
    if ((await kicker.count()) === 0) continue;
    const text = ((await kicker.first().textContent()) ?? '').replace(/\s+/g, ' ').trim();
    if (/^\d+ of \d+$/i.test(text)) bare.push(`${site.name}: "${text}"`);
  }
  expect(bare).toEqual([]);
});

for (const viewport of adapter.gating) {
  test(`UXF-017 a search for the words of an option's meaning lands on the option's row, ${viewport.name}`, async ({ page: p }) => {
    await prepare(p, viewport, adapter.themes[0]!);
    await p.goto(`${adapter.base}docs/getting-started`);
    await settle(p, adapter.notReady);
    await p.getByRole('textbox', { name: 'Search' }).click();
    await p.keyboard.type('request records');
    const result = p.getByRole('option').filter({ hasText: /reference|CLI/i }).first();
    await expect(result).toBeVisible({ timeout: 5000 });
    await result.click();
    await expect(p).toHaveURL(/docs\/reference\/cli/);
    await settle(p, adapter.notReady);
    const row = p.locator('tr', { has: p.locator('td:first-child > code', { hasText: /^--observe-requests\b/ }) });
    await expect(row.locator('td:first-child > code')).toBeInViewport({ ratio: 1 });
    await expect(p).toHaveURL(/#run--observe-requests$/);
  });
}

test('UXF-021 a keyboard reader who tabs past the search field keeps focus when the search index arrives', async ({ page: p, context }) => {
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let asked = false;
  await context.route(/\/search-index[^/]*\.json/, async (route) => {
    asked = true;
    await held;
    await route.continue();
  });
  await prepare(p, desktop, adapter.themes[0]!);
  await p.goto(`${adapter.base}docs/concepts/observe`);
  await settle(p, adapter.notReady);
  const field = p.locator('input.navbar__search-input');
  for (let tabs = 0; tabs < 40 && !(await field.evaluate((el) => el === document.activeElement)); tabs += 1) await p.keyboard.press('Tab');
  await expect(field).toBeFocused();
  await p.keyboard.press('Tab');
  const away = await p.evaluate(() => document.activeElement?.outerHTML.slice(0, 120) ?? 'none');
  expect(away).not.toMatch(/navbar__search-input/);
  await expect.poll(() => asked).toBe(true);
  release();
  await expect(p.locator('input.navbar__search-input[role="combobox"]')).toHaveCount(1, { timeout: 10_000 });
  await p.waitForTimeout(300);
  expect(await p.evaluate(() => document.activeElement?.outerHTML.slice(0, 120) ?? 'none')).toBe(away);
});
