// L1: the gate verdicts, the metric functions and the bot's choices, without a browser.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { carries, choose, glanceOf, normal, onThisPage, rank, revealer } from './bot.ts';
import { judge, stale } from './gates.ts';
import type { Mark } from './gates.ts';
import { median, p95, pageMetrics, taskMetrics, tolerance, UNREACHED } from './metrics.ts';
import type { PageRecord, PlayRecord } from './metrics.ts';
import { candidates, CONTROLS } from './probe.ts';
import type { Candidate } from './probe.ts';

const where = { scenario: 's', viewport: '1280x800', theme: 'dark' };
const mark: Mark = { gate: 'gate-1', scenario: 's', viewport: '*', theme: '*', finding: 'UXF-001' };

test('a gate fails an unmarked failure and passes a clean gate', () => {
  assert.equal(judge('gate-1', where, ['top: document'], []).ok, false);
  assert.equal(judge('gate-1', where, [], []).ok, true);
});

test('a marked failure passes with its finding; a mark that covers nothing in a run is stale', () => {
  assert.deepEqual([judge('gate-1', where, ['x'], [mark]).ok, judge('gate-1', where, ['x'], [mark]).finding], [true, 'UXF-001']);
  assert.deepEqual(stale([mark], [judge('gate-1', where, ['x'], [mark])]), []);
  assert.deepEqual(stale([mark], [judge('gate-1', where, [], [mark])]), [mark]);
});

test('a mark with a match covers only the failures that contain it', () => {
  const search: Mark = { ...mark, gate: 'gate-2', match: 'search', finding: 'UXF-001' };
  const toggle: Mark = { ...search, match: 'toggle', finding: 'UXF-002' };
  assert.equal(judge('gate-2', where, ['top: search absent'], [search]).ok, true);
  assert.equal(judge('gate-2', where, ['top: search absent', 'top: theme toggle absent'], [search]).ok, false);
  assert.equal(judge('gate-2', where, ['top: theme toggle absent'], [search, toggle]).finding, 'UXF-002');
  assert.equal(judge('gate-2', { ...where, scenario: 'any' }, ['top: search absent'], [{ ...search, scenario: '*' }]).ok, true);
  const run = [judge('gate-2', where, ['top: theme toggle absent'], [search, toggle])];
  assert.deepEqual(stale([search, toggle], run), [search]);
});

test('a mark of another gate, scenario or viewport does not apply', () => {
  assert.equal(judge('gate-2', where, ['x'], [mark]).ok, false);
  assert.equal(judge('gate-1', { ...where, scenario: 't' }, ['x'], [mark]).ok, false);
  assert.equal(judge('gate-1', where, ['x'], [{ ...mark, viewport: '390x844' }]).ok, false);
});

test('median and p95', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([1, 2, 3, 4]), 2.5);
  assert.equal(median([]), 0);
  assert.equal(p95([1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 3]), 1);
  assert.equal(p95([1, 3]), 3);
});

const play = (over: Partial<PlayRecord>): PlayRecord => ({ task: 't', viewport: 'v', theme: 'light', mode: 'pointer', reached: true, actions: 2, glances: [1, 2], detours: 0, tabs: 0, path: [], stuck: null, ...over });

test('task metrics take the worst play, looks from glances and tabs from keyboard plays', () => {
  const m = taskMetrics([play({}), play({ actions: 5, glances: [1, 3, 3, 3, 1], detours: 2 }), play({ mode: 'keyboard', tabs: 40, actions: 99 })]);
  assert.deepEqual(m, { 'screen_only.actions': 5, 'screen_only.looks': 11, 'screen_only.detours': 2, 'glance_cost.median': 3, 'glance_cost.p95': 3, 'keyboard.tabs': 40 });
});

const record = (over: Partial<PageRecord>): PageRecord => ({ page: 'p', viewport: 'v', theme: 'light', overflow: 0, hidden: 0, axe: 0, scroll_boxes: 0, layout_shift: 0, words: 100, long_paragraphs: 0, max_paragraph: 40, ...over });

test('an unreached play counts as the cap: more actions than allowed, every glance 3, every Tab', () => {
  const m = taskMetrics([play({ reached: false, actions: 4, glances: [1, 1, 1, 1] }), play({ mode: 'keyboard', reached: false, tabs: 9 })]);
  assert.deepEqual([m['screen_only.actions'], m['screen_only.looks'], m['glance_cost.median'], m['keyboard.tabs']], [UNREACHED.actions, UNREACHED.actions * 3, 3, UNREACHED.tabs]);
});

test('page metrics keep axe per theme and the worst of the rest', () => {
  const m = pageMetrics([record({ axe: 7, hidden: 2 }), record({ theme: 'dark', axe: 2, overflow: 3, hidden: 2 }), record({ hidden: 1, long_paragraphs: 2 })]);
  assert.deepEqual(m, { 'axe.light': 7, 'axe.dark': 2, 'overflow.elements': 3, 'always_visible.misses': 3, 'layout_shift.score': 0, 'reading_load.words': 100, 'reading_load.long_paragraphs': 2, 'reading_load.max_paragraph': 40 });
});

test('layout shift: the worst page check, 0.01 absolute tolerance, and missing where one check could not read it', () => {
  assert.equal(pageMetrics([record({ layout_shift: 0.0123 }), record({ theme: 'dark', layout_shift: 0.2 })])['layout_shift.score'], 0.2);
  assert.ok(Number.isNaN(pageMetrics([record({}), record({ layout_shift: null })])['layout_shift.score']));
  assert.equal(tolerance('layout_shift.score'), 0.01);
});

const cand = (over: Partial<Candidate>): Candidate => ({ index: 0, role: 'link', name: '', href: null, context: '', box: { x: 0, y: 0, width: 10, height: 10 }, inView: true, covered: false, navigation: false, ...over });

test('a word is carried only at the start of a word, any case', () => {
  assert.equal(carries('Restart', 'start'), false);
  assert.equal(carries('Getting started', 'start'), true);
  assert.equal(carries('Getting started', 'getting started'), true);
  assert.equal(carries('Serving over HTTP', 'http'), true);
  assert.equal(carries('Rally', 'run'), false);
  assert.equal(carries('run --observe', '--observe'), true);
});

test('the bot ranks by word order, name before heading', () => {
  const words = ['install', 'build'];
  assert.equal(rank({ name: 'Build', context: '' }, words), 2);
  assert.equal(rank({ name: 'Other', context: 'Install' }, words), 1);
  assert.equal(rank({ name: 'Install it', context: '' }, words), 0);
  assert.equal(rank({ name: 'Rally', context: 'Three' }, words), -1);
});

test('the bot chooses only usable, unused controls that carry a word', () => {
  const all = [
    cand({ name: 'Build', href: 'h1', inView: false }),
    cand({ name: 'Build', href: 'h2', covered: true }),
    cand({ name: 'Build', href: 'h3' }),
    cand({ name: 'Install', href: 'h4' }),
    cand({ name: 'Rally', href: 'h5' }),
  ];
  assert.equal(choose(all, ['install', 'build'], new Set(), 'here')?.href, 'h4');
  assert.equal(choose(all, ['install', 'build'], new Set(['h4']), 'here')?.href, 'h3');
  assert.equal(choose(all, ['install', 'build'], new Set(['h4', 'h3']), 'here'), null);
  assert.equal(choose(all, ['install'], new Set(), 'h4'), null);
});

test('a search result on the page the reader is on is skipped; one elsewhere is chosen', () => {
  const here = cand({ role: 'option', name: '…a topology… Documentation › Concepts › Observe › The observe API' });
  const there = cand({ role: 'option', name: '…a spindle-topology/1 graph… CLI › Reference › CLI reference › run' });
  assert.equal(onThisPage(here, 'Observe'), true);
  assert.equal(onThisPage(there, 'Observe'), false);
  assert.equal(onThisPage(cand({ name: 'Observe' }), 'Observe'), false);
  assert.equal(choose([here, there], ['topology'], new Set(), 'x', 'Observe')?.name, there.name);
});

test('the bot reveals hidden navigation once per page', () => {
  const all = [cand({ role: 'button', name: 'Toggle navigation bar' })];
  const reveal = [{ role: 'button', name: 'Toggle navigation bar' }];
  assert.equal(revealer(all, reveal, new Set(), '/a')?.name, 'Toggle navigation bar');
  assert.equal(revealer(all, reveal, new Set(['/a|button|Toggle navigation bar']), '/a'), null);
});

test('glances: navigation 1, elsewhere in view 2, out of view 3', () => {
  assert.deepEqual([glanceOf({ inView: true, navigation: true }), glanceOf({ inView: true, navigation: false }), glanceOf({ inView: false, navigation: true })], [1, 2, 3]);
});

test('pages compare without .html or a trailing slash', () => {
  assert.equal(normal('/spindle/docs/'), '/spindle/docs');
  assert.equal(normal('/spindle/docs/status.html'), '/spindle/docs/status');
});

test('the keyboard pick and the candidates read the same control list', () => {
  assert.ok(candidates.toString().includes(CONTROLS));
});
