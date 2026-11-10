// The search index has one entry per heading, so a hit on an option of a reference table opens the
// section heading, screens above the row. When the search opens a page (`?_highlight=` words), this
// moves on to the anchored table row (`src/rehype/option-anchors.ts`) that carries every searched
// word, preferring rows of the section the hit named.

const norm = (text: string): string => text.toLowerCase().replace(/\s+/g, ' ');

/** The index of the first text that holds the words as typed, else of the first that holds each word, or -1. */
export function rowFor(texts: readonly string[], words: readonly string[]): number {
  const wanted = words.map(norm).filter(Boolean);
  if (wanted.length === 0) return -1;
  const phrase = texts.findIndex((text) => norm(text).includes(wanted.join(' ')));
  return phrase >= 0 ? phrase : texts.findIndex((text) => wanted.every((w) => norm(text).includes(w)));
}

const inSection = (row: Element, heading: Element | null): boolean => {
  if (!heading) return true;
  if (!(heading.compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING)) return false;
  const next = Array.from(document.querySelectorAll('article h2, article h3')).find((h) => heading.compareDocumentPosition(h) & Node.DOCUMENT_POSITION_FOLLOWING);
  return !next || Boolean(row.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING);
};

function moveToRow(): void {
  const words = new URLSearchParams(window.location.search).getAll('_highlight');
  if (words.length === 0) return;
  const rows = Array.from(document.querySelectorAll<HTMLElement>('article tr[id]'));
  const hash = decodeURIComponent(window.location.hash.slice(1));
  const heading = hash ? document.getElementById(hash) : null;
  if (heading?.tagName === 'TR') return;
  const ordered = [...rows.filter((r) => inSection(r, heading)), ...rows.filter((r) => !inSection(r, heading))];
  const row = ordered[rowFor(ordered.map((r) => r.textContent ?? ''), words)];
  if (!row) return;
  window.history.replaceState(window.history.state, '', `${window.location.pathname}${window.location.search}#${row.id}`);
  row.scrollIntoView({ block: 'start' });
}

export function onRouteDidUpdate(): void {
  // After the theme's own scroll to the hash and the search plugin's highlighting.
  window.setTimeout(moveToRow, 150);
}
