// The screen-only reader (SKILL.md § 7, docs variant): deterministic code, no model. It uses only
// controls fully inside the viewport and not covered at their centre, named as a sighted reader
// reads them; a click, a key press or a screen of scrolling is one action. From the task's start
// page it follows the visible control whose name (or section heading) carries the best of the
// task's words, and after a link that carried the first word it reads on (scrolls) to the end of
// what it led to. With no match it opens a control that reveals hidden navigation once per page,
// closes revealed navigation that covers the page, scrolls one screen down, and at a page's end
// tries the adapter's generic way into the docs once. Off the site it steers back; a link that
// opens a new tab is read and closed. It is done when the truth's text is inside the viewport on
// the truth's page, or the truth's link off the site is followed.
// With a search field (the adapter's `search`) and no control on screen carrying any task word,
// once hidden navigation has been revealed and closed, it clicks the field and types the first
// word (two actions, once per play, before scrolling), then reads the results as controls of role
// `option`, skipping a result on the page it is on; with no result it presses Escape (one action).
// By keyboard a result is reached with the arrow keys, counted with the Tabs.

import type { Page } from '@playwright/test';
import { candidates, focused, textGlance } from './probe.ts';
import type { Candidate } from './probe.ts';
import { UNREACHED } from './metrics.ts';
import type { PlayRecord } from './metrics.ts';
import { assertTheme, prepare, settle } from './page.ts';
import type { Adapter, Control, Glance, Task, Theme, Viewport } from './types.ts';

/** Most actions one task may take. */
export const MAX_ACTIONS = UNREACHED.actions - 1;
/** Most Tab presses to reach one control by keyboard. */
export const MAX_TABS = UNREACHED.tabs;

export type Mode = 'pointer' | 'keyboard';

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** `word` occurs in `text` at the start of a word, ignoring case. */
export const carries = (text: string, word: string): boolean => new RegExp(`(^|[^\\p{L}\\p{N}])${escape(word)}`, 'iu').test(text);

/** The rank of the best task word a control's name or its section heading carries; -1 for none. */
export function rank(c: Pick<Candidate, 'name' | 'context'>, words: readonly string[]): number {
  for (let i = 0; i < words.length; i += 1) if (carries(c.name, words[i]!)) return i * 2;
  for (let i = 0; i < words.length; i += 1) if (carries(c.context, words[i]!)) return i * 2 + 1;
  return -1;
}

export const usable = (c: Candidate): boolean => c.inView && !c.covered;

/** What a control leads to: its target for a link, else its role and name. */
export const target = (c: Pick<Candidate, 'href' | 'role' | 'name'>): string => c.href ?? `${c.role}|${c.name}`;

export const glanceOf = (c: Pick<Candidate, 'inView' | 'navigation'>): Glance => (!c.inView ? 3 : c.navigation ? 1 : 2);

/** The search field, by its name; once focused, an autocomplete field takes the role `combobox`. */
export const isSearchField = (c: Pick<Candidate, 'role' | 'name'>, search: Control): boolean =>
  c.name === search.name && (c.role === search.role || c.role === 'combobox');

/** A search result on the page the reader is on (its breadcrumb carries the page's title as a segment) leads nowhere new. */
export const onThisPage = (c: Pick<Candidate, 'role' | 'name'>, title: string): boolean =>
  c.role === 'option' && title !== '' && c.name.split('›').some((s) => s.trim().toLowerCase() === title.trim().toLowerCase());

/** The next control for the task: the best word match among usable controls not used before. */
export function choose(all: readonly Candidate[], words: readonly string[], used: ReadonlySet<string>, here: string, title = ''): Candidate | null {
  const open = all.filter((c) => usable(c) && !used.has(target(c)) && c.href !== here && !onThisPage(c, title));
  const ranked = open.map((c) => ({ c, r: rank(c, words) })).filter((x) => x.r >= 0);
  ranked.sort((a, b) => a.r - b.r);
  return ranked[0]?.c ?? null;
}

/** The first reveal control on screen not yet used on this page. */
export function revealer(all: readonly Candidate[], reveal: readonly Control[], used: ReadonlySet<string>, page: string): Candidate | null {
  for (const r of reveal) {
    const c = all.find((x) => usable(x) && x.role === r.role && x.name === r.name);
    if (c && !used.has(`${page}|${r.role}|${r.name}`)) return c;
  }
  return null;
}

/** A path below the base without `.html` or a trailing slash, for comparing pages. */
export const normal = (path: string): string => path.replace(/\.html$/, '').replace(/\/+$/, '');

/** Most arrow presses to move a search result's cursor onto one result. */
const MAX_ARROWS = 12;

/**
 * One action on a control; returns the keys pressed to reach it by keyboard (Tab, or an arrow for
 * a search result). `focusOnly` reaches a field without pressing anything on it.
 */
async function act(page: Page, pick: Candidate, mode: Mode, focusOnly = false): Promise<number> {
  if (mode === 'pointer') {
    await page.mouse.click(pick.box.x + pick.box.width / 2, pick.box.y + pick.box.height / 2);
    // A finger leaves no hover behind: park the pointer off the page, so nothing stays revealed by it.
    await page.mouse.move(0, 0);
    return 0;
  }
  await page.evaluate((index) => {
    document.querySelectorAll('[data-ux-pick]').forEach((el) => el.removeAttribute('data-ux-pick'));
    document.querySelectorAll('a[href], button, input, [role=button], [role=link], [role=tab], [role=option], summary')[index]?.setAttribute('data-ux-pick', '');
  }, pick.index);
  if (pick.role === 'option') {
    // A search result is reached from the field with the arrow keys, as a listbox is.
    for (let arrows = 1; arrows <= MAX_ARROWS; arrows += 1) {
      await page.keyboard.press('ArrowDown');
      const on = await page.evaluate(() => {
        const el = document.querySelector('[data-ux-pick]');
        return !!el && (el.getAttribute('aria-selected') === 'true' || /cursor|selected/i.test(el.className));
      });
      if (on) {
        await page.keyboard.press('Enter');
        return arrows;
      }
    }
    throw new Error(`no arrow press reaches option "${pick.name}" in ${MAX_ARROWS} presses`);
  }
  for (let tabs = 1; tabs <= MAX_TABS; tabs += 1) {
    await page.keyboard.press('Tab');
    if (await page.evaluate(() => document.activeElement?.hasAttribute('data-ux-pick') ?? false)) {
      if (focusOnly) return tabs;
      const f = await page.evaluate(focused);
      await page.keyboard.press(f.tag === 'input' ? 'Space' : 'Enter');
      return tabs;
    }
  }
  throw new Error(`no Tab reaches ${pick.role} "${pick.name}" in ${MAX_TABS} presses`);
}

export async function play(page: Page, adapter: Adapter, task: Task, viewport: Viewport, theme: Theme, mode: Mode): Promise<PlayRecord> {
  await prepare(page, viewport, theme);
  const record: PlayRecord = { task: task.id, viewport: viewport.name, theme: theme.name, mode, reached: false, actions: 0, glances: [], detours: 0, tabs: 0, path: [], stuck: null };
  const basePath = new URL(adapter.base).pathname;
  const truthPath = task.truth.path === null ? null : normal(`${basePath}${task.truth.path}`);
  const startPath = normal(`${basePath}${task.start}`);
  const detours = new Set<string>();
  const used = new Set<string>();
  /** The search field is used once per play. */
  let searched = false;
  /** Links that open a new tab: the URL it opened, read and closed; the reader stays on the page. */
  const opened: string[] = [];
  page.context().on('page', async (tab) => {
    await tab.waitForLoadState('domcontentloaded').catch(() => undefined);
    opened.push(tab.url());
    await tab.close().catch(() => undefined);
  });
  let reachedOff = false;
  let reading = false;
  const onSite = () => page.url().startsWith(adapter.base);
  const pathNow = () => normal(new URL(page.url()).pathname);
  const arrived = async (): Promise<boolean> => {
    if (task.truth.href) return reachedOff || page.url() === task.truth.href;
    if (!onSite() || pathNow() !== truthPath) return false;
    const g = await page.evaluate(textGlance, { text: task.truth.text, navigation: adapter.navigation });
    return g === 1 || g === 2;
  };
  const step = async (pick: Candidate, glance: Glance, note: string) => {
    record.glances.push(glance);
    const before = opened.length;
    record.tabs += await act(page, pick, mode);
    record.actions += 1;
    record.path.push(`${pick.role} "${pick.name}"${note}`);
    await settle(page, adapter.notReady);
    for (let i = 0; i < 50 && opened.length === before && pick.href && !pick.href.startsWith(adapter.base); i += 1) await page.waitForTimeout(20);
    for (const url of opened.slice(before)) {
      if (url === task.truth.href) reachedOff = true;
      else detours.add(url);
    }
    if (onSite()) {
      const now = pathNow();
      if (now !== startPath && now !== truthPath) detours.add(now);
    } else if (!(await arrived())) detours.add(page.url());
  };
  /** Once a page has been read to its end: a generic way into the docs, revealing the menu that holds it first. */
  const explore = async (all: Candidate[]): Promise<boolean> => {
    const find = (list: Candidate[]) => list.find((c) => usable(c) && adapter.explore.some((e) => e.role === c.role && e.name === c.name) && !used.has(target(c)));
    let ex = find(all);
    if (!ex) {
      const reveal = revealer(all, adapter.reveal, used, `${pathNow()}|explore`);
      if (!reveal) return false;
      used.add(`${pathNow()}|explore|${reveal.role}|${reveal.name}`);
      await step(reveal, glanceOf(reveal), ' (reveal)');
      ex = find(await page.evaluate(candidates, adapter.navigation));
      if (!ex) return true;
    }
    used.add(target(ex));
    await step(ex, glanceOf(ex), ' (explore)');
    return true;
  };
  try {
    await page.goto(`${adapter.base}${task.start}`);
    await settle(page, adapter.notReady);
    await assertTheme(page, theme);
    while (record.actions < MAX_ACTIONS) {
      if (await arrived()) {
        record.reached = true;
        break;
      }
      if (!onSite()) {
        record.glances.push(2);
        record.actions += 1;
        record.path.push('back (off the site)');
        await page.goBack();
        await settle(page, adapter.notReady);
        continue;
      }
      if (reading) {
        const y = await page.evaluate(() => window.scrollY);
        await page.evaluate(() => window.scrollBy(0, Math.round(window.innerHeight * 0.85)));
        await settle(page, adapter.notReady);
        if ((await page.evaluate(() => window.scrollY)) > y) {
          record.glances.push(3);
          record.actions += 1;
          record.path.push('scroll (reading)');
          continue;
        }
        reading = false;
      }
      const here = page.url();
      const all = await page.evaluate(candidates, adapter.navigation);
      const title = await page.evaluate(() => (document.querySelector('main h1, article h1, h1')?.textContent ?? '').replace(/\s+/g, ' ').trim());
      const pick = choose(all, task.words, used, here, title);
      if (pick) {
        used.add(target(pick));
        const from = page.url();
        await step(pick, glanceOf(pick), '');
        // A link that carried the reader's first word is followed by reading what it led to.
        reading = rank(pick, task.words) === 0 && page.url() !== from && onSite();
        continue;
      }
      const reveal = revealer(all, adapter.reveal, used, pathNow());
      if (reveal) {
        used.add(`${pathNow()}|${reveal.role}|${reveal.name}`);
        await step(reveal, glanceOf(reveal), ' (reveal)');
        continue;
      }
      const close = all.find((c) => usable(c) && adapter.dismiss.some((d) => d.role === c.role && d.name === c.name));
      if (close) {
        await step(close, glanceOf(close), ' (dismiss)');
        continue;
      }
      // No control on screen or in revealed navigation carries any task word: a reader with a
      // search field types the first word there, once per play, before scrolling.
      const field = adapter.search ? all.find((c) => usable(c) && isSearchField(c, adapter.search!)) : undefined;
      if (field && !searched && !pick) {
        searched = true;
        record.glances.push(glanceOf(field));
        record.tabs += await act(page, field, mode, true);
        if (mode === 'pointer') await page.waitForTimeout(50);
        record.actions += 1;
        record.path.push(`${field.role} "${field.name}" (search)`);
        await page.keyboard.type(task.words[0]!);
        record.glances.push(1);
        record.actions += 1;
        record.path.push(`type "${task.words[0]}"`);
        await page.waitForSelector('[role=option]', { timeout: 3000 }).catch(() => undefined);
        await settle(page, adapter.notReady);
        const results = await page.evaluate(candidates, adapter.navigation);
        if (!choose(results.filter((c) => c.role === 'option'), task.words, used, here, title)) {
          // Nothing found: the reader closes the search and goes on as without it.
          await page.keyboard.press('Escape');
          await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
          record.glances.push(1);
          record.actions += 1;
          record.path.push('Escape (no result off this page)');
          await settle(page, adapter.notReady);
        }
        continue;
      }
      const y = await page.evaluate(() => window.scrollY);
      const more = await page.evaluate(() => window.scrollY + window.innerHeight < document.documentElement.scrollHeight - 2);
      if (more) {
        await page.evaluate(() => window.scrollBy(0, Math.round(window.innerHeight * 0.85)));
        await settle(page, adapter.notReady);
      }
      if (more && (await page.evaluate(() => window.scrollY)) > y) {
        record.glances.push(3);
        record.actions += 1;
        record.path.push('scroll');
        continue;
      }
      if (await explore(all)) continue;
      record.stuck = `nothing on screen carries ${task.words.map((w) => `"${w}"`).join(', ')} at the end of ${pathNow()}`;
      break;
    }
    if (!record.reached && !record.stuck) record.stuck = `not there after ${MAX_ACTIONS} actions`;
  } catch (error) {
    record.stuck = error instanceof Error ? error.message.split('\n')[0]! : String(error);
  }
  record.detours = detours.size;
  return record;
}
