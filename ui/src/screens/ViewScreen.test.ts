import { mount } from '@vue/test-utils';
import { describe, expect, test } from 'vitest';
import ViewScreen from './ViewScreen.vue';
import * as fixtures from '../../dev/observe-fixtures';
import type { ViewAnswer, ViewRow } from '../api/types';

const views = fixtures.views();

const rowsOf = (answer: ViewAnswer): ViewRow[] =>
  answer === null ? [] : Array.isArray(answer) ? answer : 'rows' in answer && Array.isArray(answer.rows) ? (answer.rows as ViewRow[]) : [answer as ViewRow];

describe('view', () => {
  test('the_view_screen_renders_a_declared_view_as_a_table', () => {
    for (const [component, declared] of Object.entries(views)) {
      for (const [view, answer] of Object.entries(declared)) {
        const wrapper = mount(ViewScreen, { props: { component, view, answer } });
        const rows = rowsOf(answer);
        const body = wrapper.findAll('[data-test="view-table"] tbody tr');
        expect(body, `${component} ${view}`).toHaveLength(rows.length);
        const columns = wrapper.findAll('[data-test="view-table"] thead th').map((th) => th.text());
        expect(columns).toEqual(Object.keys(rows[0]!));
      }
    }
  });

  test('an answer as the instance ingress gives it, rows in a list, is one table row per row', () => {
    const rows = [
      { note_id: 'n-1', text: 'first' },
      { note_id: 'n-2', text: 'second' },
      { note_id: 'n-3', text: 'third' },
    ];
    const wrapper = mount(ViewScreen, { props: { component: 'pad', view: 'notes.pad.AllNotes', answer: { rows } } });
    const body = wrapper.findAll('[data-test="view-table"] tbody tr');
    expect(body.map((tr) => tr.findAll('td').map((td) => td.text()))).toEqual(
      rows.map((row) => [row.note_id, row.text]),
    );
  });
});
