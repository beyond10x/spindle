// How the panel's topology screen draws `GET /__spindle/topology`: top-down, one row per kind of
// node, the ingress alone on the first; each row keeps the order the topology lists its nodes in.
// No chart library: plain SVG coordinates (docs/design/observe.md § 6).

import type { EdgeTraffic, Topology, TopologyEdge, TopologyNode } from '../api/types';

export const NODE = { width: 190, height: 26 };
const GAP = { x: 18, y: 84 };
const MARGIN = 16;

/** The row of each kind of node, the ingress first. */
const ROW: Record<TopologyNode['kind'], number> = {
  ingress: 0,
  specification: 1,
  domain: 2,
  command: 3,
  view: 3,
  external: 4,
  event: 4,
  binding: 5,
};

export interface Placed {
  node: TopologyNode;
  x: number;
  y: number;
}

export interface Drawn {
  edge: TopologyEdge;
  d: string;
  label: { x: number; y: number };
}

export interface Drawing {
  nodes: Placed[];
  edges: Drawn[];
  width: number;
  height: number;
}

/** Where each node and edge of `topology` is drawn. */
export function draw(topology: Topology): Drawing {
  const rows: TopologyNode[][] = [];
  for (const node of topology.nodes) {
    const row = ROW[node.kind] ?? Math.max(...Object.values(ROW)) + 1;
    (rows[row] ??= []).push(node);
  }
  const widest = Math.max(1, ...rows.map((row) => row?.length ?? 0));
  const width = 2 * MARGIN + widest * NODE.width + (widest - 1) * GAP.x;
  const nodes: Placed[] = [];
  let y = MARGIN;
  for (const row of rows) {
    if (!row?.length) continue;
    const used = row.length * NODE.width + (row.length - 1) * GAP.x;
    const left = (width - used) / 2;
    row.forEach((node, index) => nodes.push({ node, x: Math.round(left + index * (NODE.width + GAP.x)), y }));
    y += NODE.height + GAP.y;
  }
  const at = new Map(nodes.map((placed) => [placed.node.id, placed]));
  const edges = topology.edges.flatMap((edge) => {
    const from = at.get(edge.from);
    const to = at.get(edge.to);
    return from && to ? [curve(edge, from, to)] : [];
  });
  return { nodes, edges, width, height: y - GAP.y + MARGIN };
}

/** A curve from the side of `from` that faces `to` into the side of `to` that faces `from`. */
function curve(edge: TopologyEdge, from: Placed, to: Placed): Drawn {
  const down = to.y >= from.y;
  const sx = from.x + NODE.width / 2;
  const sy = down ? from.y + NODE.height : from.y;
  const tx = to.x + NODE.width / 2;
  const ty = down ? to.y : to.y + NODE.height;
  const middle = Math.round((sy + ty) / 2);
  return {
    edge,
    d: `M ${sx} ${sy} C ${sx} ${middle}, ${tx} ${middle}, ${tx} ${ty}`,
    label: { x: Math.round((sx + tx) / 2), y: middle },
  };
}

/** A node's name without its qualifier, `OpenTask` for `tasks.board.OpenTask`; a domain's whole. */
export const short = (node: TopologyNode): string =>
  node.kind === 'domain' ? node.name : node.name.slice(node.name.lastIndexOf('.') + 1);

/** The status mix of a `routes` edge: `2xx 1 · 4xx 1`. */
export const statusMix = (traffic: EdgeTraffic): string =>
  traffic.statuses.map((status) => `${status.status_class} ${status.count}`).join(' · ');

/** The answer sources of an `asks` edge: `file 3 · default 1`. */
export const sourceMix = (traffic: EdgeTraffic): string =>
  traffic.sources.map((source) => `${source.source} ${source.count}`).join(' · ');

/** The latest requests of a `routes` edge, newest first: `#4 #0`. */
export const latestCalls = (traffic: EdgeTraffic): string => traffic.latest.map((sequence) => `#${sequence}`).join(' ');
