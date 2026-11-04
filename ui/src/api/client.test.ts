import { describe, expect, test } from 'vitest';
import { Client, tokenFrom } from './client';
import type { Sample, StateChange } from './types';

type Call = { url: string; init: RequestInit | undefined };

const fake = (answer: (url: string) => Response) => {
  const calls: Call[] = [];
  const fetcher: typeof fetch = async (input, init) => {
    const url = String(input);
    calls.push({ url, init });
    return answer(url);
  };
  return { calls, fetcher };
};

const json = (value: unknown) =>
  new Response(JSON.stringify(value), { headers: { 'content-type': 'application/json' } });

const bearer = (call: Call | undefined) => new Headers(call?.init?.headers).get('authorization');

describe('the API client', () => {
  test('reads the token from the page URL', () => {
    expect(tokenFrom('http://127.0.0.1:7341/?t=abc123#/instance/x')).toBe('abc123');
    expect(tokenFrom('http://127.0.0.1:7341/')).toBe('');
  });

  test('sends every request with the token as a bearer credential', async () => {
    const { calls, fetcher } = fake(() => json({}));
    const client = new Client({ base: 'http://127.0.0.1:7341', token: 'abc123', fetch: fetcher });
    await client.deployment();
    await client.catalogue('rally-table');
    await client.view('rally-table', 'rally.table.MatchState');
    expect(calls.map((call) => call.url)).toEqual([
      'http://127.0.0.1:7341/api/deployment',
      'http://127.0.0.1:7341/api/catalogue?component=rally-table',
      'http://127.0.0.1:7341/api/views/rally-table/rally.table.MatchState',
    ]);
    expect(calls.map(bearer)).toEqual(['Bearer abc123', 'Bearer abc123', 'Bearer abc123']);
    expect(calls.every((call) => (call.init?.method ?? 'GET') === 'GET')).toBe(true);
  });

  test('a refused request is an error naming the path and the status', async () => {
    const { fetcher } = fake(() => new Response('no', { status: 401 }));
    const client = new Client({ base: '', token: 'wrong', fetch: fetcher });
    await expect(client.deployment()).rejects.toThrow('GET /api/deployment answered 401');
  });

  test('the stream hands over samples and state changes as they arrive', async () => {
    const sample = { instance_id: 'i-1', sequence: 1 } as Sample;
    const change: StateChange = { instance_id: 'i-1', from: 'Placed', to: 'Running', at: 't' };
    const body = `event: sample\ndata: ${JSON.stringify(sample)}\n\nevent: state\ndata: ${JSON.stringify(change)}\n\n`;
    const { calls, fetcher } = fake(
      () => new Response(body, { headers: { 'content-type': 'text/event-stream' } }),
    );
    const client = new Client({ base: '', token: 'abc123', fetch: fetcher });
    const samples: Sample[] = [];
    const changes: StateChange[] = [];
    await client.stream({ sample: (s) => samples.push(s), state: (c) => changes.push(c) });
    expect(calls[0]?.url).toBe('/api/stream');
    expect(bearer(calls[0])).toBe('Bearer abc123');
    expect(samples).toEqual([sample]);
    expect(changes).toEqual([change]);
  });
});
