// What the topology screen draws: one card per component, one edge per binding, and where each
// goes. The hub (the component with the most bindings) stands on the left, the rest on the right;
// larger systems fall back to a grid.

import type { Binding, Deployment } from '../api/types';

export const CARD = { width: 300, height: 220 };
const GAP = { x: 220, y: 40 };
const MARGIN = { x: 20, y: 70 };
export const CLOCK = 'clock';

export interface Placed {
  name: string;
  x: number;
  y: number;
}

export interface Edge {
  binding: Binding;
  from: string;
  to: string;
  path: string;
  label: { x: number; y: number };
}

export interface Layout {
  cards: Placed[];
  edges: Edge[];
  clock: { x: number; y: number } | null;
  width: number;
  height: number;
}

const short = (name: string) => name.slice(name.lastIndexOf('.') + 1);
export { short };

export function layout(deployment: Deployment): Layout {
  const names = deployment.components.map((c) => c.name);
  const degree = (name: string) =>
    deployment.bindings.filter((b) => b.from === name || b.to === name).length;
  const hub = names.reduce<string | undefined>((best, name) => (best === undefined || degree(name) > degree(best) ? name : best), undefined);

  const at = new Map<string, Placed>();
  if (names.length <= 5 && hub !== undefined) {
    at.set(hub, { name: hub, x: MARGIN.x, y: MARGIN.y });
    names
      .filter((n) => n !== hub)
      .forEach((name, row) =>
        at.set(name, { name, x: MARGIN.x + CARD.width + GAP.x, y: MARGIN.y + row * (CARD.height + GAP.y) }),
      );
  } else {
    names.forEach((name, i) =>
      at.set(name, {
        name,
        x: MARGIN.x + (i % 3) * (CARD.width + GAP.x),
        y: MARGIN.y + Math.floor(i / 3) * (CARD.height + GAP.y),
      }),
    );
  }
  const cards = names.map((n) => at.get(n)!);
  const periodic = deployment.bindings.some((b) => b.kind === 'periodic' || b.from === undefined);
  const clock = periodic ? { x: MARGIN.x + CARD.width / 2, y: 20 } : null;

  const pairs = new Map<string, number>();
  const edges = deployment.bindings.map((binding) => {
    const from = binding.from ?? CLOCK;
    const key = [from, binding.to].sort().join('|');
    const index = pairs.get(key) ?? 0;
    pairs.set(key, index + 1);
    return route(binding, from, at, clock, index);
  });

  const width = Math.max(...cards.map((c) => c.x + CARD.width), CARD.width) + MARGIN.x;
  const height = Math.max(...cards.map((c) => c.y + CARD.height), CARD.height) + MARGIN.x;
  return { cards, edges, clock, width, height };
}

function route(
  binding: Binding,
  from: string,
  at: Map<string, Placed>,
  clock: Layout['clock'],
  index: number,
): Edge {
  const target = at.get(binding.to);
  const source = from === CLOCK ? null : at.get(from);
  if (!target) return { binding, from, to: binding.to, path: '', label: { x: 0, y: 0 } };
  if (!source) {
    const start = clock ?? { x: target.x + CARD.width / 2, y: 20 };
    const end = { x: target.x + CARD.width / 2, y: target.y };
    return {
      binding,
      from,
      to: binding.to,
      path: `M ${start.x} ${start.y + 8} L ${end.x} ${end.y}`,
      label: { x: start.x + 10, y: (start.y + end.y) / 2 + 4 },
    };
  }
  if (source === target) {
    const x = source.x + CARD.width;
    const y = source.y + 30 + index * 16;
    return { binding, from, to: binding.to, path: `M ${x} ${y} c 40 0 40 30 0 30`, label: { x: x + 44, y: y + 18 } };
  }
  const rightward = source.x <= target.x;
  const sx = rightward ? source.x + CARD.width : source.x;
  const tx = rightward ? target.x : target.x + CARD.width;
  const lane = (index % 2 === 0 ? -1 : 1) * (12 + 14 * Math.floor(index / 2));
  const sy = source.y + CARD.height / 2 + lane;
  const ty = target.y + CARD.height / 2 + lane;
  const mid = (sx + tx) / 2;
  return {
    binding,
    from,
    to: binding.to,
    path: `M ${sx} ${sy} C ${mid} ${sy}, ${mid} ${ty}, ${tx} ${ty}`,
    label: { x: mid, y: (sy + ty) / 2 + lane - 4 },
  };
}
