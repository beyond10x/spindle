// Lets a path or URL in a table cell wrap after each `/` (a `<wbr>`), so one long path does not
// widen its column past the article. Words still never break inside; copying the code is unchanged.

interface Node {
  type: string;
  tagName?: string;
  value?: string;
  children?: Node[];
}

const WBR: Node = { type: 'element', tagName: 'wbr', children: [] };

/** The text split after each `/` that has text after it, with a `<wbr>` at each split. */
export function breakAfterSlashes(value: string): Node[] {
  const parts = value.split(/(?<=\/)(?=[^/])/);
  return parts.flatMap((part, i) => (i === 0 ? [{ type: 'text', value: part }] : [{ ...WBR }, { type: 'text', value: part }]));
}

const breakCode = (code: Node): void => {
  code.children = (code.children ?? []).flatMap((child) => (child.type === 'text' && (child.value ?? '').includes('/') ? breakAfterSlashes(child.value ?? '') : [child]));
};

export default function rehypeCodeBreaks() {
  return (tree: Node): void => {
    const walk = (node: Node, inCell: boolean): void => {
      for (const child of node.children ?? []) {
        if (child.type !== 'element') continue;
        if (inCell && child.tagName === 'code') breakCode(child);
        else walk(child, inCell || child.tagName === 'td' || child.tagName === 'th');
      }
    };
    walk(tree, false);
  };
}
