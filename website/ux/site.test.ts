// L1: the site's own render steps (`website/src`), without a browser.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { rowFor } from '../src/client/search-row.ts';
import { breakAfterSlashes } from '../src/rehype/code-breaks.ts';
import { labelTable } from '../src/rehype/table-labels.ts';

test('a search moves to the row holding the words as typed before a row holding each word apart', () => {
  const rows = ['Record every request the listener answers; records are kept', 'How many request records --observe keeps'];
  assert.equal(rowFor(rows, ['request', 'records']), 1);
  assert.equal(rowFor(rows, ['listener', 'records']), 0);
  assert.equal(rowFor(rows, ['topology']), -1);
  assert.equal(rowFor(rows, []), -1);
});

test('a path wraps after each slash with text after it, and its text is unchanged', () => {
  const parts = breakAfterSlashes('ess/fixtures/observe/');
  assert.deepEqual(parts.map((p) => p.tagName ?? p.value), ['ess/', 'wbr', 'fixtures/', 'wbr', 'observe/']);
  assert.deepEqual(breakAfterSlashes('--observe').map((p) => p.value), ['--observe']);
});

test('each body cell takes its column header as its label; a one-column table is left alone', () => {
  const cell = (tag: string, value: string) => ({ type: 'element', tagName: tag, children: [{ type: 'text', value }] });
  const row = (tag: string, ...values: string[]) => ({ type: 'element', tagName: 'tr', children: values.map((v) => cell(tag, v)) });
  const table = (head: string[], body: string[]) => ({
    type: 'element',
    tagName: 'table',
    children: [
      { type: 'element', tagName: 'thead', children: [row('th', ...head)] },
      { type: 'element', tagName: 'tbody', children: [row('td', ...body)] },
    ],
  });
  const two = table(['Argument', 'Meaning'], ['--port', 'The port']);
  assert.equal(labelTable(two), 2);
  assert.deepEqual(two.children[1]!.children[0]!.children.map((c) => (c as { properties?: { dataLabel?: string } }).properties?.dataLabel), ['Argument', 'Meaning']);
  assert.equal(labelTable(table(['Only'], ['x'])), 0);
});
