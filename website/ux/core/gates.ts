// Hard-gate verdicts (SKILL.md § 4, docs variant), with known-failing marks: a gate that fails on
// today's site is marked with the finding that records it, so the loop can start. A mark covers
// the failures that contain its `match` (all of them without one) and must keep covering one
// somewhere in a run: `stale` names the marks that covered none, and the round command fails on
// them until they are removed (a wildcard mark cannot be judged stale by one verdict alone).
// Gate 3 (unmissable completion) is the truth in view at the end of gate 4 on a docs site; gate 6
// (cue truth) has no guidance cue to hold.

export const GATES = {
  'gate-1': 'no horizontal scroll, nothing past its box',
  'gate-2': 'always visible',
  'gate-4': 'screen-only reader reaches the truth',
  'gate-5': 'accessibility (axe)',
  'gate-5-keyboard': 'accessibility (keyboard only)',
} as const;

export type Gate = keyof typeof GATES;

export interface Mark {
  gate: Gate;
  /** A scenario (a task id or a page name), or `*`. */
  scenario: string;
  /** A viewport name, or `*`. */
  viewport: string;
  /** A theme name, or `*`. */
  theme: string;
  /** Text a failure contains to be covered by this mark; every failure without one. */
  match?: string;
  finding: string;
}

export interface Where {
  scenario: string;
  viewport: string;
  theme: string;
}

export interface Verdict {
  gate: Gate;
  where: Where;
  passed: boolean;
  ok: boolean;
  finding: string | null;
  failures: string[];
  message: string;
}

const any = (pattern: string, value: string) => pattern === '*' || pattern === value;

export const applicable = (marks: readonly Mark[], gate: Gate, where: Where): Mark[] =>
  marks.filter((m) => m.gate === gate && any(m.scenario, where.scenario) && any(m.viewport, where.viewport) && any(m.theme, where.theme));

const covers = (m: Mark, failure: string) => m.match === undefined || failure.includes(m.match);

export function judge(gate: Gate, where: Where, failures: readonly string[], marks: readonly Mark[]): Verdict {
  const mine = applicable(marks, gate, where);
  const passed = failures.length === 0;
  const at = `${where.scenario} ${where.viewport} ${where.theme}`;
  const uncovered = failures.filter((f) => !mine.some((m) => covers(m, f)));
  const findings = [...new Set(mine.filter((m) => failures.some((f) => covers(m, f))).map((m) => m.finding))];
  let message: string;
  let ok: boolean;
  if (uncovered.length) {
    ok = false;
    message = `${gate} (${GATES[gate]}) fails at ${at}:\n  ${uncovered.slice(0, 12).join('\n  ')}`;
  } else if (passed) {
    ok = true;
    message = `${gate} passes at ${at}`;
  } else {
    ok = true;
    message = `${gate} known-failing at ${at} (${findings.join(', ')}): ${failures.length} failure(s)`;
  }
  return { gate, where, passed, ok, finding: findings.join(', ') || null, failures: [...failures], message };
}

/** Marks that covered no failure of any verdict of a run, among the verdicts given. */
export function stale(marks: readonly Mark[], verdicts: readonly Verdict[]): Mark[] {
  return marks.filter((m) => !verdicts.some((v) => v.gate === m.gate && applicable([m], v.gate, v.where).length > 0 && v.failures.some((f) => covers(m, f))));
}
