// The loop's generic core, docs-site variant: what an adapter supplies. Nothing under `ux/core/`
// imports the site or the adapter (`ux/core/core.test.ts` holds it).

export interface Viewport {
  name: string;
  width: number;
  height: number;
}

export interface Theme {
  name: string;
  colorScheme: 'light' | 'dark';
}

/** Where a control or a text is when the reader looks for it: in a navigation region, elsewhere in view, not in view. */
export type Glance = 1 | 2 | 3;

/** A control found by its accessible role and name only. */
export interface Control {
  role: string;
  name: string;
}

/** One page of the sitemap, by its path below the base URL (`''` is the landing page). */
export interface SitePage {
  name: string;
  path: string;
  /** The page is written by a generator and never edited by hand. */
  generated: boolean;
}

export interface Truth {
  /** The page that answers the task, below the base URL; or null when the answer is off the site. */
  path: string | null;
  /** Text of the answer that must be inside the viewport once the task is done. */
  text: string;
  /** For an answer off the site: the link the reader activates to reach it. */
  href?: string;
  /** The anchor of the answer on its page, for the report. */
  anchor?: string;
}

/** A findability task: a reader's question, the words they scan the screen for, and its truth. */
export interface Task {
  id: string;
  question: string;
  /** The page the reader starts on, below the base URL. */
  start: string;
  /** Words from the question the reader scans for, in order of preference. */
  words: string[];
  truth: Truth;
}

/** A scenario card: a persona, the tasks they bring, and the planned twists. */
export interface Card {
  id: string;
  anchor: boolean;
  persona: string;
  tasks: string[];
  twists: string[];
}

/** One always-visible element, at the viewports it applies to. */
export interface Required {
  name: string;
  selector: string;
  /** Only on pages that have it (a docs sidebar is not on the landing page). */
  onlyWhere?: string;
}

export interface Adapter {
  name: string;
  /** The base URL the site is served under, ending in `/`. */
  base: string;
  gating: Viewport[];
  measured: Viewport[];
  themes: Theme[];
  pages: SitePage[];
  tasks: Task[];
  cards: Card[];
  /** The always-visible elements at one viewport. */
  alwaysVisible(viewport: Viewport): Required[];
  /** Navigation regions: a control in one costs a glance of 1. */
  navigation: string[];
  /** The article of a page, for the reading load. */
  article: string;
  /** Controls that reveal hidden navigation (a menu, an in-page table of contents), tried when nothing matches. */
  reveal: Control[];
  /** Controls that close revealed navigation covering the page, used when it holds no match. */
  dismiss: Control[];
  /** The generic ways into the docs a reader tries, once each, when a page has been read to its end without a match. */
  explore: Control[];
  /** A selector present until the page is interactive (hydrated). */
  notReady: string;
  /** The site's search field, if it has one: the reader types the task's first word into it when no control on screen carries that word. Its results are controls of role `option`. */
  search?: Control;
}
