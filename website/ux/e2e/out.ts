import { join } from 'node:path';

/** Where a run writes its page checks, plays, verdicts and shots (git-ignored). */
export const OUT = process.env.UX_OUT ?? join(import.meta.dirname, '..', '.out', 'run');
