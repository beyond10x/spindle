// Labels each body cell of a Markdown table with its column's header (`data-label`), so a narrow
// screen can show a row as label and value pairs (`src/css/custom.css`). Rendering only: the
// Markdown, generated or hand-written, is unchanged.

interface Node {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: Node[];
}

const text = (node: Node): string => (node.type === 'text' ? (node.value ?? '') : (node.children ?? []).map(text).join(''));

const isElement = (node: Node, ...tags: string[]): boolean => node.type === 'element' && tags.includes(node.tagName ?? '');

const childrenOf = (node: Node | undefined, ...tags: string[]): Node[] => (node?.children ?? []).filter((c) => isElement(c, ...tags));

/** Sets `data-label` on every body cell of one table from its header row; returns the cells labelled. */
export function labelTable(table: Node): number {
  const head = childrenOf(table, 'thead')[0];
  const headers = childrenOf(childrenOf(head, 'tr')[0], 'th', 'td').map((c) => text(c).replace(/\s+/g, ' ').trim());
  if (headers.length < 2 || headers.every((h) => h === '')) return 0;
  let n = 0;
  for (const body of childrenOf(table, 'tbody')) {
    for (const row of childrenOf(body, 'tr')) {
      childrenOf(row, 'td', 'th').forEach((cell, i) => {
        const label = headers[i];
        if (!label) return;
        cell.properties = { ...(cell.properties ?? {}), dataLabel: label };
        n += 1;
      });
    }
  }
  return n;
}

export default function rehypeTableLabels() {
  return (tree: Node): void => {
    const walk = (node: Node): void => {
      for (const child of node.children ?? []) {
        if (isElement(child, 'table')) labelTable(child);
        else if (child.type === 'element') walk(child);
      }
    };
    walk(tree);
  };
}
