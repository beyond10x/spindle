import assert from 'node:assert/strict';
import { test } from 'node:test';
import { TOLERANCES, tolerance } from './metrics.ts';
import { check, update } from './ratchet.ts';
import type { Baseline, Measured } from './ratchet.ts';

const KEYS = ['screen_only.actions', 'glance_cost.median', 'reading_load.words'];
const values = (over: Record<string, number> = {}): Record<string, number> => ({ ...Object.fromEntries(KEYS.map((k) => [k, 2])), ...over });
const baseline = (m: Record<string, number>, decision: Record<string, string> = {}): Baseline => ({
  about: 'test',
  tolerances: TOLERANCES,
  scenarios: { s: Object.fromEntries(Object.entries(m).map(([k, v]) => [k, { value: v, tolerance: tolerance(k), ...(decision[k] ? { decision: decision[k] } : {}) }])) },
});
const kinds = (b: Baseline, m: Measured, main: Baseline | null = null) => check(b, m, main).map((v) => [v.kind, v.key]);

test('ratchet passes when every value equals its baseline', () => {
  assert.deepEqual(check(baseline(values()), { s: values() }, baseline(values())), []);
});

test('ratchet fails a value worse than its tolerance, not one within it', () => {
  assert.deepEqual(kinds(baseline(values()), { s: values({ 'screen_only.actions': 3, 'glance_cost.median': 2.09 }) }), [['worse', 'screen_only.actions']]);
});

test('ratchet fails an improvement not taken into the baseline', () => {
  assert.deepEqual(kinds(baseline(values()), { s: values({ 'reading_load.words': 1 }) }), [['improved-not-taken', 'reading_load.words']]);
});

test('ratchet fails a missing value on either side', () => {
  const partial = values();
  delete partial['glance_cost.median'];
  assert.deepEqual(kinds(baseline(values()), { s: partial }), [['missing', 'glance_cost.median']]);
  assert.equal(check(baseline(values()), { s: values(), t: values() }, null).filter((v) => v.scenario === 't').length, KEYS.length);
});

test('ratchet fails a baseline worse than main without a decision id, and passes with one', () => {
  const looser = values({ 'screen_only.actions': 5 });
  assert.deepEqual(kinds(baseline(looser), { s: looser }, baseline(values())), [['baseline-worse-without-decision', 'screen_only.actions']]);
  assert.deepEqual(check(baseline(looser, { 'screen_only.actions': 'R-01/decision' }), { s: looser }, baseline(values())), []);
});

test('ratchet update takes improvements in, and a worse value only with a decision id', () => {
  const old = baseline(values());
  const now = { s: values({ 'reading_load.words': 1, 'screen_only.actions': 9 }) };
  const kept = update(old, now);
  assert.equal(kept.scenarios.s?.['reading_load.words']?.value, 1);
  assert.equal(kept.scenarios.s?.['screen_only.actions']?.value, 2);
  const decided = update(old, now, 'R-01/decision');
  assert.deepEqual(decided.scenarios.s?.['screen_only.actions'], { value: 9, tolerance: 0, decision: 'R-01/decision' });
  assert.deepEqual(check(decided, now, old), []);
});

test('keyboard.tabs holds a 30 % band both ways: run-to-run Tab counts on one site span 170 to 215', () => {
  const old = baseline({ 'keyboard.tabs': 200 });
  assert.equal(tolerance('keyboard.tabs'), 0.3);
  assert.deepEqual(kinds(old, { s: { 'keyboard.tabs': 255 } }), []);
  assert.deepEqual(kinds(old, { s: { 'keyboard.tabs': 145 } }), []);
  assert.deepEqual(kinds(old, { s: { 'keyboard.tabs': 261 } }), [['worse', 'keyboard.tabs']]);
  assert.deepEqual(kinds(old, { s: { 'keyboard.tabs': 139 } }), [['improved-not-taken', 'keyboard.tabs']]);
  assert.deepEqual(update(old, { s: { 'keyboard.tabs': 230 } }).scenarios.s?.['keyboard.tabs'], { value: 200, tolerance: 0.3 });
  assert.equal(update(old, { s: { 'keyboard.tabs': 120 } }).scenarios.s?.['keyboard.tabs']?.value, 120);
});
