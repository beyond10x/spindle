import { describe, expect, test } from 'vitest';
import { HEIGHT, WIDTH, board, isMatchState } from './board';
import { views } from '../../dev/observe-fixtures';

const view = {
  match_id: 'm-1',
  state: 'InPlay',
  ball_x: 3,
  ball_y: 2,
  west_paddle_y: 4,
  east_paddle_y: 7,
  west_needs: 5,
  east_needs: 2,
  winner: null,
};

describe('the rally board', () => {
  test('is drawn for the MatchState view only', () => {
    expect(isMatchState('rally.table.MatchState')).toBe(true);
    expect(isMatchState('rally.west.PlayerState')).toBe(false);
  });

  test('places the ball and both paddles on a 16 by 9 grid', () => {
    expect([WIDTH, HEIGHT]).toEqual([16, 9]);
    expect(board(view)).toEqual({
      ok: true,
      board: { ball: { x: 3, y: 2 }, west: 4, east: 7, westNeeds: 5, eastNeeds: 2, winner: null },
    });
  });

  test('names the winner once there is one, and only a non-empty text is one', () => {
    expect(board({ ...view, winner: 'East' })).toMatchObject({ board: { winner: 'East' } });
    expect(board({ ...view, winner: '' })).toMatchObject({ board: { winner: null } });
    expect(board({ ...view, winner: 3 })).toMatchObject({ board: { winner: null } });
  });

  test('refuses an answer it cannot draw, with the reason the terminal frame gives', () => {
    const { ball_x: _ballX, ...missing } = view;
    expect(board(missing)).toEqual({ ok: false, error: 'the view answers no integer `ball_x`' });
    expect(board({ ...view, ball_y: 9 })).toEqual({
      ok: false,
      error: '`ball_y` is 9, outside the grid',
    });
    expect(board({ ...view, east_paddle_y: 1.5 })).toEqual({
      ok: false,
      error: 'the view answers no integer `east_paddle_y`',
    });
  });

  test('draws the fixture MatchState', () => {
    expect(board(views()['rally-table']!['rally.table.MatchState']!)).toMatchObject({ ok: true });
  });
});
