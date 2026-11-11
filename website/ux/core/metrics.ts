// The metric catalogue, docs variant: one function per key of `baseline.json` (SKILL.md § 6).
// Every key is lower-is-better. Pure: the page checks and the bot record, these functions read.

import type { Glance } from './types.ts';

/** What the screen-only reader did for one task at one viewport in one theme and one input mode. */
export interface PlayRecord {
  task: string;
  viewport: string;
  theme: string;
  mode: 'pointer' | 'keyboard';
  reached: boolean;
  actions: number;
  /** One glance per action: where the control (or the text scrolled to) was. */
  glances: Glance[];
  /** Pages visited that are neither the start nor the truth page. */
  detours: number;
  /** Tab presses (keyboard mode). */
  tabs: number;
  path: string[];
  stuck: string | null;
}

/** What one page showed at one viewport in one theme. */
export interface PageRecord {
  page: string;
  viewport: string;
  theme: string;
  overflow: number;
  /** Always-visible elements not fully in view, summed over the checked states. */
  hidden: number;
  /** Serious and critical axe-core violation nodes; null where axe did not run. */
  axe: number | null;
  scroll_boxes: number;
  /** Layout shift (the sum of `layout-shift` entries without recent input) from load to the page's end; null where not read. */
  layout_shift: number | null;
  words: number;
  long_paragraphs: number;
  max_paragraph: number;
}

export const TASK_KEYS = [
  'screen_only.actions',
  'screen_only.looks',
  'screen_only.detours',
  'glance_cost.median',
  'glance_cost.p95',
  'keyboard.tabs',
] as const;

export const PAGE_KEYS = [
  'axe.light',
  'axe.dark',
  'overflow.elements',
  'always_visible.misses',
  'layout_shift.score',
  'reading_load.words',
  'reading_load.long_paragraphs',
  'reading_load.max_paragraph',
] as const;

export type TaskKey = (typeof TASK_KEYS)[number];
export type PageKey = (typeof PAGE_KEYS)[number];
export type MetricKey = TaskKey | PageKey;
export type Metrics = Partial<Record<MetricKey, number>>;

/** Steps: tolerance 5 % relative. Layout shift: 0.01 absolute. Tabs: 30 % relative, both ways. Everything else is a count: tolerance 0. */
export const STEP_KEYS: ReadonlySet<string> = new Set(['glance_cost.median', 'glance_cost.p95']);

export const SHIFT_KEYS: ReadonlySet<string> = new Set(['layout_shift.score']);

/** Tab counts vary run to run on one site (release 170 to 215, install 17 to 21: R-02, T5); they hold a band both ways. */
export const BAND_KEYS: ReadonlySet<string> = new Set(['keyboard.tabs']);

export const TOLERANCES = { counts: 0, steps: 0.05, layout_shift: 0.01, tabs: 0.3 } as const;

export const tolerance = (key: string): number =>
  STEP_KEYS.has(key) ? TOLERANCES.steps : SHIFT_KEYS.has(key) ? TOLERANCES.layout_shift : BAND_KEYS.has(key) ? TOLERANCES.tabs : TOLERANCES.counts;

export function median(values: readonly number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid]! : (sorted[mid - 1]! + sorted[mid]!) / 2;
}

/** Nearest-rank 95th percentile. */
export function p95(values: readonly number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(0.95 * sorted.length) - 1)]!;
}

const sum = (values: readonly number[]) => values.reduce((n, v) => n + v, 0);

/**
 * What a play that did not reach the truth counts as: more actions than the reader may take, every
 * one a glance of 3, every Tab the keyboard reader may press. A reader that gives up sooner, or
 * crashes, is then never better than one that finds the answer.
 */
export const UNREACHED = { actions: 31, glance: 3, tabs: 400 } as const;

const charged = (p: PlayRecord): PlayRecord =>
  p.reached
    ? p
    : { ...p, actions: UNREACHED.actions, glances: Array.from({ length: UNREACHED.actions }, () => UNREACHED.glance), tabs: p.mode === 'keyboard' ? UNREACHED.tabs : p.tabs };

/** A task's keys over its plays: the worst value over the viewports and themes, an unreached play charged as `UNREACHED`. */
export function taskMetrics(plays: readonly PlayRecord[]): Metrics {
  const pointer = plays.filter((p) => p.mode === 'pointer').map(charged);
  const keyboard = plays.filter((p) => p.mode === 'keyboard').map(charged);
  const worst = (f: (p: PlayRecord) => number, of: readonly PlayRecord[]) => (of.length ? Math.max(...of.map(f)) : 0);
  return {
    'screen_only.actions': worst((p) => p.actions, pointer),
    'screen_only.looks': worst((p) => sum(p.glances), pointer),
    'screen_only.detours': worst((p) => p.detours, pointer),
    'glance_cost.median': worst((p) => median(p.glances), pointer),
    'glance_cost.p95': worst((p) => p95(p.glances), pointer),
    'keyboard.tabs': worst((p) => p.tabs, keyboard),
  };
}

/** A page's keys over its checks at the gating viewports: the worst value; axe per theme; always-visible misses summed over the viewports. */
export function pageMetrics(records: readonly PageRecord[]): Metrics {
  const worst = (f: (r: PageRecord) => number, of: readonly PageRecord[] = records) => (of.length ? Math.max(...of.map(f)) : 0);
  return {
    'axe.light': worst((r) => r.axe ?? 0, records.filter((r) => r.theme === 'light')),
    'axe.dark': worst((r) => r.axe ?? 0, records.filter((r) => r.theme === 'dark')),
    'overflow.elements': worst((r) => r.overflow),
    // Summed over the viewports, worst theme: a miss at any one viewport moves it.
    'always_visible.misses': Math.max(0, ...[...new Set(records.map((r) => r.theme))].map((t) => sum(records.filter((r) => r.theme === t).map((r) => r.hidden)))),
    // A page whose shift could not be read has no value: the ratchet fails it as missing.
    'layout_shift.score': records.some((r) => r.layout_shift === null) ? Number.NaN : Math.round(worst((r) => r.layout_shift ?? 0) * 1000) / 1000,
    'reading_load.words': worst((r) => r.words),
    'reading_load.long_paragraphs': worst((r) => r.long_paragraphs),
    'reading_load.max_paragraph': worst((r) => r.max_paragraph),
  };
}

/** Visible words in a text: runs of letters or digits. */
export const words = (text: string): number => (text.match(/[\p{L}\p{N}]+/gu) ?? []).length;
