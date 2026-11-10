// Gives each option row of a reference table an anchor, so a link can land on the row itself:
// a row whose first cell starts with an inline-code flag (`--observe`) under the section `## run`
// gets the id `run--observe` (the section, then the flag). Rendering only: the Markdown, generated
// or hand-written, is unchanged.

interface Node {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: Node[];
}

const text = (node: Node): string => (node.type === 'text' ? (node.value ?? '') : (node.children ?? []).map(text).join(''));

const slug = (value: string): string =>
  value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9 _-]/g, '')
    .replace(/\s+/g, '-');

const isElement = (node: Node, ...tags: string[]): boolean => node.type === 'element' && tags.includes(node.tagName ?? '');

/** The flag a row's first cell starts with, as inline code: `--observe <X>` gives `--observe`. */
export function flagOf(row: Node): string | null {
  const first = (row.children ?? []).find((c) => isElement(c, 'td', 'th'));
  const lead = first?.children?.find((c) => c.type === 'element' || (c.type === 'text' && (c.value ?? '').trim() !== ''));
  if (!lead || !isElement(lead, 'code')) return null;
  const match = /^(--?[A-Za-z0-9][A-Za-z0-9-]*)/.exec(text(lead).trim());
  return match ? match[1]! : null;
}

function rows(node: Node, visit: (row: Node) => void): void {
  if (isElement(node, 'tr')) visit(node);
  for (const child of node.children ?? []) rows(child, visit);
}

export default function rehypeOptionAnchors() {
  return (tree: Node): void => {
    let section: string | null = null;
    const seen = new Set<string>();
    const walk = (node: Node): void => {
      for (const child of node.children ?? []) {
        if (isElement(child, 'h2', 'h3')) {
          section = child.properties?.id ? String(child.properties.id) : slug(text(child));
          continue;
        }
        if (isElement(child, 'table')) {
          if (!section) continue;
          rows(child, (row) => {
            const flag = flagOf(row);
            if (!flag || row.properties?.id) return;
            const id = `${section}${flag.startsWith('-') ? '' : '-'}${flag}`;
            if (seen.has(id)) return;
            seen.add(id);
            row.properties = { ...(row.properties ?? {}), id };
          });
          continue;
        }
        if (child.type === 'element') walk(child);
      }
    };
    walk(tree);
  };
}
