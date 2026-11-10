// In-page probes, passed to `page.evaluate`. Each function is self-contained: Playwright sends its
// source to the page, so none may use anything declared outside its own body.

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Overflow {
  /** `document` (horizontal page scroll), `viewport` (an element cut by the viewport's edge) or `box` (content past its own box). */
  kind: 'document' | 'viewport' | 'box';
  what: string;
  /** How far past, in CSS pixels. */
  by: number;
}

/**
 * Hard gate 1, docs variant: no horizontal page scroll and no element running past its box.
 * Vertical page scroll is allowed, and so is content inside a box that scrolls on its own
 * (a code block, a wide table in its scroll container). An element entirely outside the viewport
 * (a skip link parked off-screen) is not cut by it and does not count.
 */
export function overflow(): Overflow[] {
  const describe = (el: Element): string => {
    const id = el.id ? `#${el.id}` : '';
    const cls = typeof el.className === 'string' && el.className.trim() ? `.${el.className.trim().split(/\s+/).slice(0, 2).join('.')}` : '';
    const text = (el.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 40);
    return `${el.tagName.toLowerCase()}${id}${cls}${text ? ` "${text}"` : ''}`;
  };
  const out: Overflow[] = [];
  const root = document.scrollingElement ?? document.documentElement;
  // The viewport takes `overflow-x` from html, or from body when html leaves it visible.
  const htmlX = getComputedStyle(document.documentElement).overflowX;
  const viewportX = htmlX !== 'visible' ? htmlX : getComputedStyle(document.body).overflowX;
  if (root.scrollWidth > window.innerWidth + 1 && !/(hidden|clip)/.test(viewportX)) {
    out.push({ kind: 'document', what: 'document', by: root.scrollWidth - window.innerWidth });
  }
  const scrollsX = (el: Element) => /(auto|scroll)/.test(getComputedStyle(el).overflowX);
  const shown = (el: Element) => {
    for (let a: Element | null = el; a; a = a.parentElement) {
      const s = getComputedStyle(a);
      if (s.display === 'none' || s.visibility === 'hidden') return false;
    }
    return true;
  };
  const contained = (el: Element) => {
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) if (scrollsX(a)) return true;
    return false;
  };
  const cuts: { el: Element; by: number }[] = [];
  for (const el of Array.from(document.body.querySelectorAll('*'))) {
    if (el instanceof SVGElement && !(el instanceof SVGSVGElement)) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const style = getComputedStyle(el);
    if (style.position === 'fixed' && (r.left >= window.innerWidth || r.right <= 0)) continue;
    const cutByViewport = (r.right > window.innerWidth + 1 && r.left < window.innerWidth) || (r.left < -1 && r.right > 0);
    if (!cutByViewport || contained(el) || !shown(el)) continue;
    cuts.push({ el, by: Math.round(Math.max(r.right - window.innerWidth, -r.left)) });
  }
  // A cut element cuts its descendants too: report the outermost one.
  const cutSet = new Set(cuts.map((c) => c.el));
  for (const { el, by } of cuts) if (!el.parentElement || !cutSet.has(el.parentElement)) out.push({ kind: 'viewport', what: describe(el), by });
  // Ink past its box: a run of text, an image or a drawing that reaches past the border box of the
  // block that holds it, outside any box that scrolls. A layout idiom that widens an empty box (a
  // negative margin around padded columns) puts no ink past anything and does not count.
  const blockOf = (el: Element): Element => {
    for (let a: Element | null = el; a && a !== document.body; a = a.parentElement) {
      const d = getComputedStyle(a).display;
      if (d !== 'inline' && d !== 'contents' && !d.startsWith('inline') && !(a instanceof SVGElement && !(a instanceof SVGSVGElement))) return a;
    }
    return document.body;
  };
  const past = new Map<Element, number>();
  const clipped = (el: Element) => {
    for (let a: Element | null = el; a && a !== document.body; a = a.parentElement) if (/(hidden|clip)/.test(getComputedStyle(a).overflowX)) return true;
    return false;
  };
  const note = (holder: Element, r: DOMRect) => {
    if (r.width === 0 || r.height === 0 || scrollsX(holder) || contained(holder) || clipped(holder) || !shown(holder)) return;
    const block = blockOf(holder);
    const b = block.getBoundingClientRect();
    const by = Math.max(r.right - b.right, b.left - r.left);
    if (by > 1) past.set(block, Math.max(past.get(block) ?? 0, Math.round(by)));
  };
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!(node.nodeValue ?? '').trim() || !node.parentElement || /^(SCRIPT|STYLE|NOSCRIPT)$/.test(node.parentElement.tagName)) continue;
    range.selectNodeContents(node);
    for (const r of Array.from(range.getClientRects())) note(node.parentElement, r);
  }
  for (const el of Array.from(document.body.querySelectorAll('img, svg, video, canvas, iframe'))) {
    if (el instanceof SVGElement && el.parentElement?.closest('svg')) continue;
    note(el.parentElement ?? el, el.getBoundingClientRect());
  }
  for (const [block, by] of past) out.push({ kind: 'box', what: describe(block), by });
  return out;
}

/** Boxes that scroll horizontally on their own: allowed, reported. */
export function scrollBoxes(): number {
  let n = 0;
  for (const el of Array.from(document.body.querySelectorAll('*'))) {
    const style = getComputedStyle(el);
    if (/(auto|scroll)/.test(style.overflowX) && el.scrollWidth > el.clientWidth + 1 && el.getBoundingClientRect().width > 0) n += 1;
  }
  return n;
}

export interface Visibility {
  name: string;
  selector: string;
  ok: boolean;
  why: string;
}

/** Hard gate 2: each selector's first shown element is inside the viewport, not clipped and not covered. */
export function fullyVisible(required: { name: string; selector: string }[]): Visibility[] {
  const out: Visibility[] = [];
  for (const { name, selector } of required) {
    const found = Array.from(document.querySelectorAll(selector)).filter((el) => {
      const r = el.getBoundingClientRect();
      const s = getComputedStyle(el);
      return r.width > 0 && r.height > 0 && s.visibility !== 'hidden';
    });
    const el = found[0];
    if (!el) {
      out.push({ name, selector, ok: false, why: 'absent or not shown' });
      continue;
    }
    const r = el.getBoundingClientRect();
    let why = '';
    if (r.left < -0.5 || r.top < -0.5 || r.right > window.innerWidth + 0.5 || r.bottom > window.innerHeight + 0.5) {
      why = `outside the viewport (${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(r.height)})`;
    }
    for (let a = el.parentElement; !why && a && a !== document.body; a = a.parentElement) {
      const style = getComputedStyle(a);
      if (style.overflowX === 'visible' && style.overflowY === 'visible') continue;
      const c = a.getBoundingClientRect();
      if (r.left < c.left - 0.5 || r.top < c.top - 0.5 || r.right > c.right + 0.5 || r.bottom > c.bottom + 0.5) {
        why = `clipped by ${a.tagName.toLowerCase()}`;
      }
    }
    if (!why) {
      const inset = 2;
      const points: [number, number][] = [
        [(r.left + r.right) / 2, (r.top + r.bottom) / 2],
        [r.left + inset, r.top + inset],
        [r.right - inset, r.top + inset],
        [r.left + inset, r.bottom - inset],
        [r.right - inset, r.bottom - inset],
      ];
      for (const [x, y] of points) {
        const hit = document.elementFromPoint(x, y);
        if (hit && hit !== el && !el.contains(hit) && !hit.contains(el)) {
          why = `covered at ${Math.round(x)},${Math.round(y)} by ${hit.tagName.toLowerCase()}`;
          break;
        }
      }
    }
    out.push({ name, selector, ok: !why, why: why || 'in view' });
  }
  return out;
}

/** The text a reader can see without scrolling: text nodes inside the viewport and every clipping box. */
export function visibleText(): string {
  const parts: string[] = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  const clippedOut = (r: DOMRect, el: Element | null): boolean => {
    for (let a = el; a && a !== document.body; a = a.parentElement) {
      const style = getComputedStyle(a);
      if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return true;
      if (style.overflowX === 'visible' && style.overflowY === 'visible') continue;
      const c = a.getBoundingClientRect();
      if (r.right < c.left || r.bottom < c.top || r.left > c.right || r.top > c.bottom) return true;
    }
    return false;
  };
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const value = node.nodeValue ?? '';
    if (!value.trim()) continue;
    const parent = node.parentElement;
    if (parent && /^(SCRIPT|STYLE|TITLE|NOSCRIPT)$/.test(parent.tagName)) continue;
    range.selectNodeContents(node);
    const r = range.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    if (r.bottom <= 0 || r.top >= window.innerHeight || r.right <= 0 || r.left >= window.innerWidth) continue;
    if (clippedOut(r, parent)) continue;
    parts.push(value);
  }
  return parts.join(' ').replace(/\s+/g, ' ').trim();
}

export interface Candidate {
  index: number;
  role: string;
  name: string;
  href: string | null;
  /** The heading of the section the control sits in. */
  context: string;
  box: Box;
  inView: boolean;
  covered: boolean;
  /** The control sits in a navigation region. */
  navigation: boolean;
}

export const CONTROLS = 'a[href], button, input, [role=button], [role=link], [role=tab], [role=option], summary';

/**
 * Controls a screen-only reader can see, with a role and a name computed as a sighted reader reads
 * them: `aria-label`, else the visible text, else the title. Reading order: top to bottom, then
 * left to right.
 */
export function candidates(navigation: string[]): Candidate[] {
  const query = 'a[href], button, input, [role=button], [role=link], [role=tab], [role=option], summary';
  const regions = navigation.flatMap((s) => Array.from(document.querySelectorAll(s)));
  const roleOf = (el: Element): string => {
    const explicit = el.getAttribute('role');
    if (explicit) return explicit;
    if (el.tagName === 'A') return 'link';
    if (el.tagName === 'BUTTON' || el.tagName === 'SUMMARY') return 'button';
    if (el instanceof HTMLInputElement) return el.type === 'search' || el.type === 'text' ? 'searchbox' : el.type;
    return 'generic';
  };
  // A sighted reader reads the visible text; a control with none (an icon) by its label or title.
  const nameOf = (el: Element): string => {
    if (el instanceof HTMLInputElement) return (el.placeholder || el.getAttribute('aria-label') || el.title || '').trim();
    const text = (el instanceof HTMLElement ? el.innerText : (el.textContent ?? '')).replace(/[\s​]+/g, ' ').trim();
    if (/[\p{L}\p{N}]/u.test(text)) return text;
    return (el.getAttribute('aria-label') ?? el.getAttribute('title') ?? text).trim();
  };
  const headingOf = (el: Element): string => {
    let at: Element | null = el;
    while (at && at !== document.body) {
      let sib: Element | null = at.previousElementSibling;
      while (sib) {
        if (/^H[1-4]$/.test(sib.tagName)) return (sib.textContent ?? '').replace(/\s+/g, ' ').replace(/​/g, '').trim();
        sib = sib.previousElementSibling;
      }
      at = at.parentElement;
    }
    return '';
  };
  const out: Candidate[] = [];
  Array.from(document.querySelectorAll(query)).forEach((el, index) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    for (let a: Element | null = el; a; a = a.parentElement) {
      const s = getComputedStyle(a);
      if (s.visibility === 'hidden' || s.display === 'none' || s.opacity === '0') return;
    }
    const inView = r.left >= -0.5 && r.top >= -0.5 && r.right <= window.innerWidth + 0.5 && r.bottom <= window.innerHeight + 0.5;
    const cx = (r.left + r.right) / 2;
    const cy = (r.top + r.bottom) / 2;
    const hit = inView ? document.elementFromPoint(cx, cy) : null;
    const covered = !hit || (hit !== el && !el.contains(hit) && !(hit instanceof HTMLLabelElement && hit.contains(el)));
    out.push({
      index,
      role: roleOf(el),
      name: nameOf(el),
      href: el instanceof HTMLAnchorElement ? el.href : null,
      context: headingOf(el),
      box: { x: r.left, y: r.top, width: r.width, height: r.height },
      inView,
      covered,
      navigation: regions.some((n) => n.contains(el)),
    });
  });
  return out.sort((a, b) => (Math.abs(a.box.y - b.box.y) > 4 ? a.box.y - b.box.y : a.box.x - b.box.x));
}

/** Where a text is: 1 inside a navigation region and in view, 2 elsewhere in view, 3 on the page but not in view, 0 absent. */
export function textGlance(args: { text: string; navigation: string[] }): number {
  const want = args.text.replace(/\s+/g, ' ');
  const regions = args.navigation.flatMap((s) => Array.from(document.querySelectorAll(s)));
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let best = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const holder = node.parentElement?.closest('tr, p, li, figcaption, h1, h2, h3, h4, label, pre, td, th') ?? node.parentElement;
    if (!holder) continue;
    const text = ((holder instanceof HTMLElement ? holder.innerText : holder.textContent) ?? '').replace(/\s+/g, ' ').replace(/​/g, '');
    if (!text.includes(want)) continue;
    let hidden = false;
    for (let a: Element | null = holder; a; a = a.parentElement) {
      const s = getComputedStyle(a);
      if (s.visibility === 'hidden' || s.display === 'none') hidden = true;
    }
    if (hidden) continue;
    range.selectNodeContents(node);
    const r = range.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    const inView = r.left >= -0.5 && r.top >= -0.5 && r.right <= window.innerWidth + 0.5 && r.bottom <= window.innerHeight + 0.5;
    const level = inView ? (regions.some((n) => n.contains(node)) ? 1 : 2) : 3;
    if (best === 0 || level < best) best = level;
  }
  return best;
}

export interface Reading {
  words: number;
  paragraphs: number;
  /** Paragraphs of more than 80 words. */
  long_paragraphs: number;
  max_paragraph: number;
}

/** The reading load of the page's article: words, and paragraphs by length. */
export function reading(article: string): Reading {
  const root = document.querySelector(article) ?? document.querySelector('main') ?? document.body;
  const count = (text: string) => (text.match(/[\p{L}\p{N}]+/gu) ?? []).length;
  const paragraphs = Array.from(root.querySelectorAll('p, li > p, dd')).map((p) => count((p as HTMLElement).innerText ?? p.textContent ?? ''));
  return {
    words: count((root as HTMLElement).innerText ?? root.textContent ?? ''),
    paragraphs: paragraphs.length,
    long_paragraphs: paragraphs.filter((n) => n > 80).length,
    max_paragraph: paragraphs.length ? Math.max(...paragraphs) : 0,
  };
}

/** The role and name of the focused element. */
export function focused(): { tag: string; name: string; href: string | null } {
  const el = document.activeElement;
  if (!el || el === document.body) return { tag: 'body', name: '', href: null };
  const name = el.getAttribute('aria-label') ?? (el instanceof HTMLElement ? el.innerText : (el.textContent ?? '')).replace(/\s+/g, ' ').trim();
  return { tag: el.tagName.toLowerCase(), name, href: el instanceof HTMLAnchorElement ? el.href : null };
}

/**
 * A signature of what is rendered and where the page stands, to wait until it has settled. It
 * counts the Tab stops too: a box that becomes focusable once it measures its own overflow (a wide
 * table) changes the keyboard path after the text has settled.
 */
export function signature(): string {
  const stops = document.querySelectorAll('[tabindex], a[href], button, input').length;
  return `${location.href}|${Math.round(window.scrollY)}|${document.body.innerText.length}|${stops}`;
}
