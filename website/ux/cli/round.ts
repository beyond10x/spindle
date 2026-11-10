// The round command (SKILL.md § 3.2), deterministic: reads a run's page checks, plays and verdicts
// (`ux/.out/run/`, written by the L2 and L3 specs) and writes
//
//   measure        `ux/.out/measured.json` (scenario → ratchet keys) and `ux/.out/gates.json`
//   write <R-nn>   also `ux/rounds/<R-nn>/report.md`, `round.json`, `shots/` (a contact sheet per
//                  gating viewport and theme, the worst page per viewport and theme) and a row of
//                  `ux/rounds/trend.md`
//
//   node cli/round.ts measure | write R-nn [after]   (after: the re-measure once the change set is built, into R-nn/after/)

import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join, relative, resolve } from 'node:path';
import { chromium } from '@playwright/test';
import { adapter } from '../adapter.ts';
import { GATES, judge, stale } from '../core/gates.ts';
import type { Gate, Mark, Verdict } from '../core/gates.ts';
import { median, PAGE_KEYS, pageMetrics, TASK_KEYS, taskMetrics } from '../core/metrics.ts';
import type { PageRecord, PlayRecord } from '../core/metrics.ts';
import { axeNodes } from '../core/page.ts';
import type { PageCheck } from '../core/page.ts';
import type { Baseline } from '../core/ratchet.ts';
import { OUT } from '../e2e/out.ts';

const ux = resolve(import.meta.dirname, '..');
const repo = resolve(ux, '../..');
const [command, round, stage] = process.argv.slice(2);
const label = stage ? `${round} ${stage}` : round;

const readDir = <T>(dir: string): T[] =>
  existsSync(join(OUT, dir))
    ? readdirSync(join(OUT, dir))
        .filter((f) => f.endsWith('.json'))
        .sort()
        .map((f) => JSON.parse(readFileSync(join(OUT, dir, f), 'utf8')) as T)
    : [];

const checks = readDir<PageCheck>('pages');
const measuredChecks = readDir<PageCheck>('measured');
const plays = readDir<PlayRecord>('plays');
const marks = (JSON.parse(readFileSync(join(ux, 'known-failing.json'), 'utf8')) as { marks: Mark[] }).marks;
// Judged again against today's marks, so the counts follow ux/known-failing.json as it stands.
const stored = readDir<Verdict[]>('verdicts').flat();
const verdicts = stored.map((v) => judge(v.gate, v.where, v.failures, marks));
const storedRed = stored.filter((v) => !v.ok).length;

const toRecord = (c: PageCheck): PageRecord => ({
  page: c.page,
  viewport: c.viewport,
  theme: c.theme,
  overflow: Math.max(0, ...c.states.map((s) => s.overflow.length)),
  hidden: c.states.reduce((n, s) => n + s.hidden.length, 0),
  axe: c.axe === null ? null : axeNodes(c.axe),
  scroll_boxes: c.scroll_boxes,
  layout_shift: c.layout_shift ?? null,
  words: c.reading.words,
  long_paragraphs: c.reading.long_paragraphs,
  max_paragraph: c.reading.max_paragraph,
});

const measured: Record<string, Record<string, number>> = {};
for (const task of adapter.tasks) {
  const mine = plays.filter((p) => p.task === task.id);
  if (mine.length) measured[task.id] = taskMetrics(mine) as Record<string, number>;
}
const pages = adapter.pages;
for (const site of pages) {
  const mine = checks.filter((c) => c.page === site.name).map(toRecord);
  if (mine.length) measured[`page.${site.name}`] = pageMetrics(mine) as Record<string, number>;
}

type GateCount = { pass: number; fail: number; known: number; findings: string[] };
const gates = {} as Record<Gate, GateCount>;
for (const gate of Object.keys(GATES) as Gate[]) gates[gate] = { pass: 0, fail: 0, known: 0, findings: [] };
for (const v of verdicts) {
  const c = gates[v.gate];
  if (v.passed) c.pass += 1;
  else if (v.finding) {
    c.known += 1;
    for (const f of v.finding.split(', ')) if (!c.findings.includes(f)) c.findings.push(f);
    c.findings.sort();
  } else c.fail += 1;
}

mkdirSync(join(ux, '.out'), { recursive: true });
writeFileSync(join(ux, '.out', 'measured.json'), `${JSON.stringify(measured, null, 2)}\n`);
writeFileSync(join(ux, '.out', 'gates.json'), `${JSON.stringify(gates, null, 2)}\n`);
console.log(`round: ${checks.length} page checks, ${plays.length} plays, ${verdicts.length} verdicts, ${Object.keys(measured).length} scenarios measured`);
const staleMarks = stale(marks, verdicts);
for (const m of staleMarks) console.log(`round: stale mark ${m.gate} ${m.scenario} ${m.viewport} ${m.theme}${m.match ? ` "${m.match}"` : ''} (${m.finding}) covers no failure of this run: close the finding and remove the mark`);
if (command === 'measure') process.exit(checks.length && plays.length && !staleMarks.length ? 0 : 1);
if (command !== 'write' || !round || !/^R-\d{2}$/.test(round)) {
  console.error('usage: node cli/round.ts measure | write R-nn');
  process.exit(2);
}

const dir = stage ? join(ux, 'rounds', round, stage) : join(ux, 'rounds', round);
const shots = join(dir, 'shots');
mkdirSync(shots, { recursive: true });
const commit = execFileSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: ux, encoding: 'utf8' }).trim();
const dirty = execFileSync('git', ['status', '--porcelain', '--', '..'], { cwd: ux, encoding: 'utf8' }).trim() !== '';
const date = new Date().toISOString().slice(0, 10);

/** Gate failures one page check shows, over both states and axe. */
const failuresOf = (c: PageCheck) => c.states.reduce((n, s) => n + s.overflow.length + s.hidden.length, 0) + (c.axe?.length ?? 0);

// Contact sheets: every page at each gating viewport, one sheet per viewport and theme.
const sheets: string[] = [];
const browser = await chromium.launch();
try {
  for (const viewport of adapter.gating) {
    for (const theme of adapter.themes) {
      const items = checks.filter((c) => c.viewport === viewport.name && c.theme === theme.name && c.shot && existsSync(c.shot));
      if (!items.length) continue;
      const cols = viewport.width > 800 ? 4 : 6;
      const w = Math.round(viewport.width / (viewport.width > 800 ? 4 : 2));
      const html = `<!doctype html><meta charset="utf-8"><style>body{margin:0;background:#fff;color:#000;font:12px sans-serif;display:grid;grid-template-columns:repeat(${cols},${w}px);gap:8px;padding:8px}figure{margin:0}img{width:100%;border:1px solid #888}figcaption{padding:2px 0}</style>${items
        .map((i) => `<figure><img src="file://${i.shot}"><figcaption>${i.page} · ${failuresOf(i)} gate failure(s)</figcaption></figure>`)
        .join('')}`;
      const page = await browser.newPage({ viewport: { width: cols * (w + 8) + 8, height: 400 } });
      const sheet = join(OUT, `contact-${viewport.name}-${theme.name}.html`);
      writeFileSync(sheet, html);
      await page.goto(`file://${sheet}`);
      await page.waitForLoadState('load');
      const file = `contact--${viewport.name}--${theme.name}.jpg`;
      await page.screenshot({ path: join(shots, file), fullPage: true, type: 'jpeg', quality: 75 });
      sheets.push(file);
      await page.close();
    }
  }
} finally {
  await browser.close();
}

// The worst page per gating viewport and theme: the most gate failures, then the most words.
const worst: { page: string; viewport: string; theme: string; failures: number; file: string }[] = [];
for (const viewport of adapter.gating) {
  for (const theme of adapter.themes) {
    const top = checks
      .filter((c) => c.viewport === viewport.name && c.theme === theme.name && c.shot && existsSync(c.shot))
      .sort((a, b) => failuresOf(b) - failuresOf(a) || b.reading.words - a.reading.words)[0];
    if (!top) continue;
    const file = `worst--${basename(top.shot!)}`;
    copyFileSync(top.shot!, join(shots, file));
    worst.push({ page: top.page, viewport: viewport.name, theme: theme.name, failures: failuresOf(top), file });
  }
}

const baselinePath = join(ux, 'baseline.json');
const baseline = existsSync(baselinePath) ? (JSON.parse(readFileSync(baselinePath, 'utf8')) as Baseline) : null;
// `last`: for the after stage, the same round before its change set; else the round before.
const previous = (() => {
  const n = Number(round.slice(2)) - 1;
  // The last state of a round: its after/ re-measure when it has one, else its before/, else its own.
  const last = (dir: string) => [join(dir, 'after', 'round.json'), join(dir, 'before', 'round.json'), join(dir, 'round.json')].find((f) => existsSync(f));
  const p =
    stage === 'after'
      ? [join(ux, 'rounds', round, 'before', 'round.json'), join(ux, 'rounds', round, 'round.json')].find((f) => existsSync(f))
      : n >= 0
        ? last(join(ux, 'rounds', `R-${String(n).padStart(2, '0')}`))
        : undefined;
  return p ? (JSON.parse(readFileSync(p, 'utf8')) as { measured: Record<string, Record<string, number>> }) : null;
})();

const playsOf = (task: string) => plays.filter((p) => p.task === task).sort((a, b) => `${a.viewport}${a.theme}${a.mode}`.localeCompare(`${b.viewport}${b.theme}${b.mode}`));

const roundJson = {
  round,
  date,
  commit: `${commit}${dirty ? '+uncommitted' : ''}`,
  adapter: adapter.name,
  viewports: { gating: adapter.gating.map((v) => v.name), measured: adapter.measured.map((v) => v.name) },
  themes: adapter.themes.map((t) => t.name),
  cards: adapter.cards,
  pages: pages.map((p) => ({ ...p })),
  page_checks: checks.length,
  plays: plays.length,
  gates,
  measured,
  tasks: Object.fromEntries(
    adapter.tasks.map((t) => [
      t.id,
      {
        question: t.question,
        truth: t.truth,
        plays: playsOf(t.id).map((p) => ({ viewport: p.viewport, theme: p.theme, mode: p.mode, reached: p.reached, actions: p.actions, looks: p.glances.reduce((n, g) => n + g, 0), tabs: p.tabs, detours: p.detours, path: p.path, stuck: p.stuck })),
      },
    ]),
  ),
  measured_only: measuredChecks.map((c) => ({ page: c.page, viewport: c.viewport, theme: c.theme, overflow: Math.max(0, ...c.states.map((s) => s.overflow.length)), hidden: c.states.reduce((n, s) => n + s.hidden.length, 0), axe: axeNodes(c.axe) })),
  worst_pages: worst,
  levels: {
    L0: 'dropped: a docs site has no guidance cue',
    L1: 'npm test in website/ux (core and findings)',
    L2: 'run: every sitemap page, gating viewports gated, measured-only viewports recorded',
    L3: 'run: every task, pointer and keyboard, at the gating viewports',
    L4: 'not run: paid, operator-only',
    L5: 'not run: paid, operator-only',
    L6: `not run: no people in ${round}; the cards were played by the screen-only reader`,
    L7: 'two fresh review agents, started after this report is written: review-heuristics.md and review-report.md in the round dir once they have run',
    L8: 'not run: per milestone',
  },
};
writeFileSync(join(dir, 'round.json'), `${JSON.stringify(roundJson, null, 2)}\n`);

const cell = (n: number | undefined) => (n === undefined ? '–' : String(n));
const lines: string[] = [];
lines.push(`# ${label} round report`, '');
lines.push(`Date ${date}, commit \`${roundJson.commit}\`. ${adapter.name}. Hermetic: the built site served from \`website/build\` on 127.0.0.1, every request off the site answered by a stub page; no keys.`);
lines.push(`Page checks: ${checks.length} (${pages.length} sitemap pages × ${adapter.gating.length} gating viewports × ${adapter.themes.length} themes), measured-only: ${measuredChecks.length}. Screen-only plays: ${plays.length} (${adapter.tasks.length} tasks × ${adapter.gating.length} viewports × ${adapter.themes.length} themes × pointer and keyboard).`, '');
lines.push('## Hard gates', '', `Verdicts over every gating check, judged again against \`ux/known-failing.json\` as it stands; known-failing = covered by a mark with its open finding. As run, ${storedRed} of ${stored.length} verdicts were red (against the marks of that moment). Site measured: ${process.env.UX_BUILD ? 'a build outside the tree (UX_BUILD)' : 'website/build'}.`, '', '| gate | what | pass | fail | known-failing | findings |', '|---|---|---|---|---|---|');
for (const [gate, c] of Object.entries(gates)) lines.push(`| ${gate} | ${GATES[gate as Gate]} | ${c.pass} | ${c.fail} | ${c.known} | ${c.findings.join(', ') || '–'} |`);
lines.push('| gate-3 | unmissable completion | – | – | – | docs: the truth in view at the end of gate 4 |', '| gate-6 | cue truth | – | – | – | dropped: no guidance cue |', '');

lines.push('## Per page, viewport and theme', '', 'Elements past their box (worst state), always-visible misses (top + end), axe serious/critical nodes.', '');
lines.push(`| page | ${adapter.gating.flatMap((v) => adapter.themes.map((t) => `${v.name} ${t.name}`)).join(' | ')} |`, `|---|${adapter.gating.flatMap(() => adapter.themes.map(() => '---|')).join('')}`);
for (const site of pages) {
  const row = adapter.gating.flatMap((v) =>
    adapter.themes.map((t) => {
      const c = checks.find((x) => x.page === site.name && x.viewport === v.name && x.theme === t.name);
      if (!c) return '–';
      const r = toRecord(c);
      return `${r.overflow} / ${r.hidden} / ${r.axe ?? '–'}`;
    }),
  );
  lines.push(`| ${site.name}${site.generated ? ' (generated)' : ''} | ${row.join(' | ')} |`);
}
lines.push('');

lines.push('## Screen-only reader per task', '', 'Pointer plays; `tabs` from the keyboard play at the same viewport and theme.', '');
lines.push('| task | card | viewport | theme | reached | actions | looks | detours | keyboard reached | tabs | path |', '|---|---|---|---|---|---|---|---|---|---|---|');
for (const t of adapter.tasks) {
  const card = adapter.cards.find((c) => c.tasks.includes(t.id));
  for (const p of playsOf(t.id).filter((x) => x.mode === 'pointer')) {
    const k = plays.find((x) => x.task === t.id && x.viewport === p.viewport && x.theme === p.theme && x.mode === 'keyboard');
    const path = p.path.join(' → ').replace(/(scroll → )+scroll/g, (m) => `scroll ×${m.split('→').length}`).replace(/(scroll \(reading\) → )+scroll \(reading\)/g, (m) => `read-scroll ×${m.split('→').length}`);
    lines.push(`| ${t.id} | ${card?.id ?? '–'}${card?.anchor ? ' (anchor)' : ''} | ${p.viewport} | ${p.theme} | ${p.reached} | ${p.actions} | ${p.glances.reduce((n, g) => n + g, 0)} | ${p.detours} | ${k?.reached ?? '–'} | ${k?.tabs ?? '–'} | ${path || '(none)'}${p.stuck ? ` — stuck: ${p.stuck}` : ''} |`);
  }
}
lines.push('', 'Truth per task:', '');
for (const t of adapter.tasks) lines.push(`- \`${t.id}\` "${t.question}": ${t.truth.path === null ? `off the site, ${t.truth.href}` : `\`/${t.truth.path}\` ${t.truth.anchor ?? ''}, text "${t.truth.text}"`}; words ${t.words.map((w) => `"${w}"`).join(', ')}; start \`/${t.start}\`.`);

lines.push('', '## Metrics against the bars', '', 'Every key lower-is-better; `base` is `ux/baseline.json`, `last` the previous round. Task keys: worst over the gating viewports and themes.', '');
lines.push(`| scenario | ${[...TASK_KEYS].join(' | ')} |`, `|---|${TASK_KEYS.map(() => '---|').join('')}`);
for (const t of adapter.tasks) {
  const m = measured[t.id];
  lines.push(`| ${t.id} | ${TASK_KEYS.map((k) => `${cell(m?.[k])} (base ${cell(baseline?.scenarios[t.id]?.[k]?.value)}, last ${cell(previous?.measured[t.id]?.[k])})`).join(' | ')} |`);
}
lines.push('', `| page | ${[...PAGE_KEYS].join(' | ')} |`, `|---|${PAGE_KEYS.map(() => '---|').join('')}`);
for (const site of pages) {
  const key = `page.${site.name}`;
  const m = measured[key];
  lines.push(`| ${site.name} | ${PAGE_KEYS.map((k) => `${cell(m?.[k])} (base ${cell(baseline?.scenarios[key]?.[k]?.value)})`).join(' | ')} |`);
}
const words = pages.map((p) => measured[`page.${p.name}`]?.['reading_load.words'] ?? 0);
lines.push('', `Reading load: median ${median(words)} words per page, ${pages.reduce((n, p) => n + (measured[`page.${p.name}`]?.['reading_load.long_paragraphs'] ?? 0), 0)} paragraphs over 80 words in all.`);

lines.push('', '## Worst pages', '', '| viewport | theme | page | gate failures | shot |', '|---|---|---|---|---|');
for (const w of worst) lines.push(`| ${w.viewport} | ${w.theme} | ${w.page} | ${w.failures} | [${w.file}](shots/${w.file}) |`);
lines.push('', '## Contact sheets', '');
for (const s of sheets) lines.push(`- [shots/${s}](shots/${s}): every page at the top, after load and settle.`);

lines.push('', '## Measured-only viewports', '', 'Reported, not gated: elements past their box, always-visible misses and axe serious/critical nodes, worst over the themes.', '');
if (measuredChecks.length) {
  lines.push(`| page | ${adapter.measured.map((v) => v.name).join(' | ')} |`, `|---|${adapter.measured.map(() => '---|').join('')}`);
  for (const site of pages) {
    const row = adapter.measured.map((v) => {
      const mine = measuredChecks.filter((c) => c.page === site.name && c.viewport === v.name).map(toRecord);
      if (!mine.length) return '–';
      return `${Math.max(...mine.map((r) => r.overflow))} / ${Math.max(...mine.map((r) => r.hidden))} / ${Math.max(...mine.map((r) => r.axe ?? 0))}`;
    });
    lines.push(`| ${site.name} | ${row.join(' | ')} |`);
  }
} else lines.push('Not run (`task ux:docs:round` runs them).');

lines.push('', '## Not measured in this round', '');
lines.push('- Questionnaire scores: none, no people (L6 not run); the cards were played by the screen-only reader.');
lines.push('- L5 discoverability probe misses: not run (paid, operator-only). L4 keyed replays: not applicable (a static site).');
lines.push('- L0 and gate 6: dropped, no guidance cue. Gate 3 is the truth in view at the end of gate 4.');
lines.push('- Recordings (rrweb): none; the evidence is the page checks, the plays and the shots.');
writeFileSync(join(dir, 'report.md'), `${lines.join('\n')}\n`);

const trend = join(ux, 'rounds', 'trend.md');
const header =
  '# UX trend, documentation site\n\nOne row per round. Gate columns: pass / fail / known-failing over every gating verdict.\n\n| round | date | commit | page checks | plays | gate-1 | gate-2 | gate-4 | gate-5 | keyboard | screen-only actions (sum) | looks (sum) | axe nodes (light + dark) | long paragraphs (sum) | tasks | glance cost median (max) | layout shift (max) |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n';
const g = (k: Gate) => `${gates[k].pass}/${gates[k].fail}/${gates[k].known}`;
const sum = (key: string, prefix = '') => Object.entries(measured).filter(([k]) => k.startsWith(prefix)).reduce((n, [, m]) => n + (m[key] ?? 0), 0);
const max = (key: string, prefix = '') => Math.max(0, ...Object.entries(measured).filter(([k]) => k.startsWith(prefix)).map(([, m]) => m[key] ?? 0));
const taskCount = Object.keys(measured).filter((k) => !k.startsWith('page.')).length;
const row = `| ${label} | ${date} | ${roundJson.commit} | ${checks.length} | ${plays.length} | ${g('gate-1')} | ${g('gate-2')} | ${g('gate-4')} | ${g('gate-5')} | ${g('gate-5-keyboard')} | ${sum('screen_only.actions')} | ${sum('screen_only.looks')} | ${sum('axe.light', 'page.') + sum('axe.dark', 'page.')} | ${sum('reading_load.long_paragraphs', 'page.')} | ${taskCount} | ${max('glance_cost.median')} | ${max('layout_shift.score', 'page.')} |`;
// Rows written before the last three columns existed get "–" in them.
const widen = (l: string) => (l.split('|').length - 2 < 17 ? `${l}${' – |'.repeat(17 - (l.split('|').length - 2))}` : l);
const rows = existsSync(trend) ? readFileSync(trend, 'utf8').split('\n').filter((l) => l.startsWith('| R-')).map(widen) : [];
const at = rows.findIndex((l) => l.startsWith(`| ${label} |`));
if (at >= 0) rows[at] = row;
else rows.push(row);
writeFileSync(trend, `${header}${rows.join('\n')}\n`);
console.log(`round: wrote ${relative(repo, dir)}/report.md, round.json, shots/ and ${relative(repo, trend)}`);
