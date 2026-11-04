import { expect, test } from 'vitest';
import { route } from './route';

test('the fragment names the screen', () => {
  expect(route('')).toEqual({ screen: 'topology' });
  expect(route('#/')).toEqual({ screen: 'topology' });
  expect(route('#/instance/rally-table-0')).toEqual({ screen: 'instance', instance: 'rally-table-0' });
  expect(route('#/view/rally-table/rally.table.MatchState')).toEqual({
    screen: 'view',
    component: 'rally-table',
    view: 'rally.table.MatchState',
  });
  expect(route('#/view/only-one')).toEqual({ screen: 'topology' });
});
