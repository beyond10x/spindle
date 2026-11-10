import { rmSync } from 'node:fs';
import { OUT } from './out.ts';

/** Each Playwright run starts from an empty run directory, so the round command reads only this run. */
export default function setup(): void {
  rmSync(OUT, { recursive: true, force: true });
}
