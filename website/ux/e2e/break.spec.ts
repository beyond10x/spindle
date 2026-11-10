// Break-tests: each layout-contract probe fails on a page built to break it and passes on its
// repaired twin, so a green L2 means something.

import { expect, test } from '@playwright/test';
import { candidates, fullyVisible, overflow, reading, textGlance } from '../core/probe.ts';
import { axe, shiftObserver, shiftSoFar } from '../core/page.ts';

test.use({ viewport: { width: 400, height: 300 } });

test('break: gate 1 sees a page scrolling sideways, an element cut by the viewport and ink past its box', async ({ page }) => {
  await page.setContent('<body style="margin:0"><div style="width:600px;height:20px">wide</div></body>');
  expect((await page.evaluate(overflow)).map((o) => o.kind)).toEqual(['document', 'viewport']);
  await page.setContent('<body style="margin:0;overflow-x:hidden"><p style="width:200px;white-space:nowrap">a_very_long_unbroken_identifier_that_runs_past_the_paragraph</p></body>');
  expect((await page.evaluate(overflow)).map((o) => o.kind)).toEqual(['box']);
  await page.setContent('<body style="margin:0"><div style="width:200px;margin-left:40px"><div style="margin:0 -16px"><p style="padding:0 16px">a padded row</p></div></div></body>');
  expect(await page.evaluate(overflow)).toEqual([]);
  await page.setContent('<body style="margin:0"><pre style="width:200px;overflow-x:auto">a long line inside a box that scrolls on its own, allowed</pre><a style="position:fixed;left:100%">skip</a></body>');
  expect(await page.evaluate(overflow)).toEqual([]);
});

test('break: gate 2 sees an element absent, outside, clipped or covered', async ({ page }) => {
  await page.setContent(
    '<body style="margin:0"><button id="out" style="position:absolute;top:290px">out</button>' +
      '<div style="width:50px;height:20px;overflow:hidden"><button id="clip" style="width:200px">clipped</button></div>' +
      '<button id="cov" style="position:absolute;top:100px;left:0">covered</button><div style="position:absolute;top:95px;left:0;width:200px;height:40px;background:red"></div>' +
      '<button id="ok" style="position:absolute;top:200px;left:0">ok</button><button id="gone" style="visibility:hidden">gone</button></body>',
  );
  const seen = await page.evaluate(fullyVisible, ['#out', '#clip', '#cov', '#ok', '#gone', '#absent'].map((s) => ({ name: s, selector: s })));
  expect(seen.map((v) => [v.name, v.ok])).toEqual([
    ['#out', false],
    ['#clip', false],
    ['#cov', false],
    ['#ok', true],
    ['#gone', false],
    ['#absent', false],
  ]);
});

test('break: gate 5 sees a link told apart by colour only, and passes it underlined', async ({ page }) => {
  const p = (style: string) =>
    `<html lang="en"><title>t</title><body><main><p style="color:#333">Read the <a href="#x" style="color:#3a3a3a;text-decoration:${style}">guide</a> before you start the build of the site.</p></main></body></html>`;
  await page.setContent(p('none'));
  expect((await axe(page)).map((v) => v.id)).toContain('link-in-text-block');
  await page.setContent(p('underline'));
  expect((await axe(page)).map((v) => v.id)).not.toContain('link-in-text-block');
});

test('break: the screen-only reader sees only usable controls, named as a sighted reader reads them', async ({ page }) => {
  await page.setContent(
    '<body style="margin:0"><nav><a href="#a">Install</a></nav><a href="#b" aria-label="Direct link">#</a><button aria-label="Toggle navigation bar"><svg width="10" height="10"></svg></button>' +
      '<a href="#c" style="position:absolute;top:900px">Below</a><a href="#d" style="visibility:hidden">Hidden</a></body>',
  );
  const all = await page.evaluate(candidates, ['nav']);
  expect(all.map((c) => [c.name, c.inView, c.navigation])).toEqual([
    ['Install', true, true],
    ['Direct link', true, false],
    ['Toggle navigation bar', true, false],
    ['Below', false, false],
  ]);
});

test('break: a text glance tells navigation, view, off-view and absent apart', async ({ page }) => {
  await page.setContent('<body style="margin:0"><nav><p>in the nav</p></nav><p>in view</p><p style="margin-top:900px">far below</p></body>');
  const g = (text: string) => page.evaluate(textGlance, { text, navigation: ['nav'] });
  expect([await g('in the nav'), await g('in view'), await g('far below'), await g('nowhere')]).toEqual([1, 2, 3, 0]);
});

test('break: the layout-shift observer sees a block pushed down after paint, and nothing on a still page', async ({ page }) => {
  await page.addInitScript(shiftObserver);
  await page.goto('data:text/html,<body style="margin:0"><div id="top"></div><p style="font:20px sans-serif">steady text that a late block pushes down</p></body>');
  await page.waitForTimeout(300);
  expect(await shiftSoFar(page)).toBe(0);
  await page.evaluate(() => {
    document.getElementById('top')!.style.height = '200px';
  });
  await page.waitForTimeout(300);
  expect(await shiftSoFar(page)).toBeGreaterThan(0.01);
});

test('break: the reading load counts words and paragraphs over 80 words', async ({ page }) => {
  await page.setContent(`<article><div class="markdown"><p>${'word '.repeat(81)}</p><p>short one</p></div></article>`);
  expect(await page.evaluate(reading, 'article .markdown')).toEqual({ words: 83, paragraphs: 2, long_paragraphs: 1, max_paragraph: 81 });
});
