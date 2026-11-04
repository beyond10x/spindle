// The rally board, read from the table's `MatchState` view answer, as the terminal frame
// (`crates/spindle-host/src/rally.rs`) reads it: the same fields, bounds and refusals.

import type { ViewRow } from '../api/types';

/** Columns: x from 0 to 15; the west paddle stands in column 0, the east one in column 15. */
export const WIDTH = 16;
/** Rows: y from 0 at the top to 8. */
export const HEIGHT = 9;

export interface Board {
  ball: { x: number; y: number };
  west: number;
  east: number;
  westNeeds: number;
  eastNeeds: number;
  winner: string | null;
}

export type Drawn = { ok: true; board: Board } | { ok: false; error: string };

export const isMatchState = (view: string): boolean => view === 'rally.table.MatchState';

class Refused extends Error {}

const integer = (view: ViewRow, field: string): number => {
  const value = view[field];
  if (typeof value !== 'number' || !Number.isInteger(value)) {
    throw new Refused(`the view answers no integer \`${field}\``);
  }
  return value;
};

const position = (view: ViewRow, field: string, bound: number): number => {
  const value = integer(view, field);
  if (value < 0 || value >= bound) throw new Refused(`\`${field}\` is ${value}, outside the grid`);
  return value;
};

/** A non-empty text under `winner`; anything else is no winner. */
export const winner = (view: ViewRow): string | null => {
  const side = view.winner;
  return typeof side === 'string' && side !== '' ? side : null;
};

export function board(view: ViewRow): Drawn {
  try {
    return {
      ok: true,
      board: {
        ball: { x: position(view, 'ball_x', WIDTH), y: position(view, 'ball_y', HEIGHT) },
        west: position(view, 'west_paddle_y', HEIGHT),
        east: position(view, 'east_paddle_y', HEIGHT),
        westNeeds: integer(view, 'west_needs'),
        eastNeeds: integer(view, 'east_needs'),
        winner: winner(view),
      },
    };
  } catch (error) {
    if (error instanceof Refused) return { ok: false, error: error.message };
    throw error;
  }
}
