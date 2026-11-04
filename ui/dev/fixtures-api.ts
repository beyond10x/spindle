// `pnpm dev` only: serves the observe API from the fixtures, so the UI runs without the control
// process. The token is `dev`; open the URL it prints.

import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Plugin } from 'vite';
import type { Deployment, Sample, StateChange } from '../src/api/types.ts';

const TOKEN = 'dev';
const HISTORY = 15;
const fixtures = fileURLToPath(new URL('../src/fixtures/', import.meta.url));
const read = <T>(name: string): T => JSON.parse(readFileSync(join(fixtures, name), 'utf8')) as T;

export function fixturesApi(): Plugin {
  return {
    name: 'spindle-observe-fixtures',
    apply: 'serve',
    configureServer(server) {
      server.httpServer?.once('listening', () => {
        const address = server.httpServer?.address();
        const port = typeof address === 'object' && address ? address.port : 5173;
        server.config.logger.info(`  observe fixtures: http://127.0.0.1:${port}/?t=${TOKEN}`);
      });
      server.middlewares.use('/api', (req, res) => answer(req, res));
    },
  };
}

function answer(req: IncomingMessage, res: ServerResponse) {
  if (req.headers.authorization !== `Bearer ${TOKEN}`) return send(res, 401, { error: 'token' });
  if (req.method !== 'GET') return send(res, 405, { error: 'read only' });
  const url = new URL(req.url ?? '/', 'http://127.0.0.1');
  const path = url.pathname;
  if (path === '/deployment') return send(res, 200, read<Deployment>('deployment.json'));
  if (path === '/catalogue') {
    const component = url.searchParams.get('component') ?? '';
    const file = `catalogue.${component}.json`;
    if (!/^[a-z0-9-]+$/.test(component) || !existsSync(join(fixtures, file))) {
      return send(res, 404, { error: `no component ${component}` });
    }
    return send(res, 200, read(file));
  }
  const view = /^\/views\/([^/]+)\/([^/]+)$/.exec(path);
  if (view) {
    const views = read<Record<string, Record<string, unknown>>>('views.json');
    const component = decodeURIComponent(view[1] ?? '');
    const name = decodeURIComponent(view[2] ?? '');
    const found = views[component]?.[name];
    return found === undefined ? send(res, 404, { error: `no view ${name}` }) : send(res, 200, found);
  }
  if (path === '/stream') return stream(req, res);
  return send(res, 404, { error: `no route ${path}` });
}

function send(res: ServerResponse, status: number, body: unknown) {
  res.statusCode = status;
  res.setHeader('content-type', 'application/json');
  res.end(JSON.stringify(body));
}

function stream(req: IncomingMessage, res: ServerResponse) {
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
  const deployment = read<Deployment>('deployment.json');
  const recorded = new Map(
    deployment.instances.map((i) => [i.instance_id, read<Sample[]>(`samples.${i.instance_id}.json`)]),
  );
  const emit = (event: string, data: unknown) => res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
  for (const change of read<StateChange[]>('states.json')) emit('state', change);
  let next = 0;
  const step = () => {
    for (const samples of recorded.values()) emit('sample', replay(samples, next));
    next += 1;
  };
  while (next < HISTORY) step();
  const timer = setInterval(step, 1000);
  req.on('close', () => clearInterval(timer));
}

function replay(samples: Sample[], index: number): Sample {
  const last = samples[samples.length - 1]!;
  if (index < samples.length) return samples[index]!;
  const before = samples[samples.length - 2] ?? last;
  const ahead = index - (samples.length - 1);
  const grow = (value: number, previous: number) => value + ahead * (value - previous);
  return {
    ...last,
    sequence: index + 1,
    at: new Date(Date.parse(last.at) + ahead * last.period_ms).toISOString(),
    counters: last.counters.map((c) => ({
      ...c,
      value: grow(c.value, before.counters.find((b) => b.name === c.name)?.value ?? c.value),
    })),
    histograms: last.histograms.map((h) => ({
      ...h,
      counts: h.counts.map((n, i) =>
        grow(n, before.histograms.find((b) => b.name === h.name)?.counts[i] ?? n),
      ),
    })),
    recent: samples[index % samples.length]!.recent,
    resources: {
      ...last.resources,
      cpu_seconds: grow(last.resources.cpu_seconds, before.resources.cpu_seconds),
    },
  };
}
