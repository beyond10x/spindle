// @vitest-environment node
import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { afterAll, describe, expect, test } from 'vitest';
import { conform } from '../lib/conform';
import { raw, root } from '../../dev/observe-fixtures';
import { requestLogShape, sampleShape } from './shapes';

const repo = resolve(root, '../../..');
const schemas = join(repo, 'ess/generated/jsonschema/schema/types');
const work = join(repo, 'target/adversary-ingress-ui');
mkdirSync(work, { recursive: true });
afterAll(() => rmSync(work, { recursive: true, force: true }));

const bundles = new Map<string, string>();

/** What `check:every_fixture_answer_validates_against_its_schema` answers for `document`. */
function schemaRefuses(type: string, document: unknown): boolean {
  const rootName = `spindle.observe.${type}.document`;
  let bundle = bundles.get(type);
  if (bundle === undefined) {
    bundle = join(work, `${type}.bundle.json`);
    execFileSync('ess', [
      'generate', 'schema', 'import-document',
      '--path', join(schemas, `spindle.observe.${type}.schema.json`),
      '--root', rootName, '--dialect', 'draft-2020-12', '--out', bundle,
    ]);
    bundles.set(type, bundle);
  }
  const file = join(work, `${type}-${bundles.size}-${Math.random().toString(36).slice(2)}.json`);
  writeFileSync(file, JSON.stringify(document));
  try {
    execFileSync('ess', ['generate', 'schema', 'validate-bundle', '--bundle', bundle, '--root', rootName, file], {
      stdio: 'pipe',
    });
    return false;
  } catch {
    return true;
  }
}

const scripted = () => structuredClone(raw('requests', 'scripted.json')) as Record<string, unknown>;
const tasks = () => structuredClone(raw('topology', 'tasks.json')) as {
  format: string;
  edges: { kind: string; traffic?: { statuses: { status_class: string; count: number }[] } }[];
};

describe('adversary: the ingress answers the schema refuses', () => {
  test('control: a request log with a negative count is refused by its schema', () => {
    expect(schemaRefuses('RequestLog', { ...scripted(), recorded: -1 })).toBe(true);
  });

  // story:ingress-request-log: `--observe-requests <N>`, N from 1 to 10 000. A log claiming a
  // ring of a million is no answer a run gives; `capacity <= 10000` projects to `maximum`.
  test('a request log whose ring holds more than 10 000 is refused by its schema', () => {
    expect(schemaRefuses('RequestLog', { ...scripted(), capacity: 1_000_000 })).toBe(true);
  });

  // The domain: "Always `spindle-requests/1`; a reader refuses any other", and the same for
  // `spindle-topology/1`. The schema is the reader the fixture check and the server tests use.
  test('an answer of another format is refused by its schema', () => {
    expect(schemaRefuses('RequestLog', { ...scripted(), format: 'spindle-requests/2' }), 'RequestLog').toBe(true);
    expect(schemaRefuses('Topology', { ...tasks(), format: 'spindle-topology/2' }), 'Topology').toBe(true);
  });

  // StatusCount.status_class: "`2xx`, `3xx`, `4xx` or `5xx`", the `status_class` label of
  // spindle_http_requests_total that story:live-topology compares each routes edge to.
  test('a routes edge counting a status class that does not exist is refused by its schema', () => {
    const topology = tasks();
    topology.edges[0]!.traffic!.statuses = [{ status_class: '6xx', count: 1 }];
    expect(schemaRefuses('Topology', topology)).toBe(true);
  });
});

describe('adversary: the shapes refuse what the schema refuses', () => {
  // The schema projects `recorded >= 0`, `sequence >= 0`, `status <= 599` and `Integer`; the
  // shape the UI conforms an answer to checks only `typeof value === 'number'`
  // (`ui/src/lib/conform.ts`). That gap predates this unit and is story:shape-check-numeric-bounds;
  // until it lands these cases pin today's behaviour, and each assertion flips when it does.
  test('the request log shape accepts counts its schema refuses, until story:shape-check-numeric-bounds', () => {
    const log = scripted() as { recorded: number; requests: { sequence: number; status: number }[] };
    log.recorded = -1;
    log.requests[0]!.sequence = 1.5;
    log.requests[0]!.status = 600;
    expect(schemaRefuses('RequestLog', log)).toBe(true);
    expect(
      conform(log, requestLogShape),
      'story:shape-check-numeric-bounds: the shape check accepts numbers the schema refuses; flip to not.toEqual([]) when that story lands',
    ).toEqual([]);
  });

  // The same check on the deploy API's Sample, unchanged by this unit: the gap predates it.
  test('the sample shape accepts a sequence its schema refuses, until story:shape-check-numeric-bounds', () => {
    const sample = structuredClone(raw('samples', 'rally-player-west-0', '01.json')) as { sequence: number };
    sample.sequence = -1;
    expect(schemaRefuses('Sample', sample)).toBe(true);
    expect(
      conform(sample, sampleShape),
      'story:shape-check-numeric-bounds: the shape check accepts numbers the schema refuses; flip to not.toEqual([]) when that story lands',
    ).toEqual([]);
  });
});
