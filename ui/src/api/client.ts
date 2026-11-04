// The one way the UI reaches the control process. Every request is a GET and carries the token
// from the page URL (`?t=`) as a bearer credential; the API is read only.

import { SseParser } from '../lib/sse';
import type { Catalogue, Deployment, Sample, StateChange, ViewAnswer } from './types';

export const tokenFrom = (href: string): string => new URL(href).searchParams.get('t') ?? '';

export interface ClientOptions {
  base: string;
  token: string;
  fetch?: typeof fetch;
}

export interface StreamHandlers {
  sample: (sample: Sample) => void;
  state: (change: StateChange) => void;
}

export class Client {
  private readonly base: string;
  private readonly token: string;
  private readonly fetcher: typeof fetch;

  constructor(options: ClientOptions) {
    this.base = options.base.replace(/\/$/, '');
    this.token = options.token;
    this.fetcher = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  }

  deployment(): Promise<Deployment> {
    return this.json('/api/deployment');
  }

  catalogue(component: string): Promise<Catalogue> {
    return this.json(`/api/catalogue?component=${encodeURIComponent(component)}`);
  }

  view(component: string, view: string): Promise<ViewAnswer> {
    return this.json(`/api/views/${encodeURIComponent(component)}/${encodeURIComponent(view)}`);
  }

  /** Reads `/api/stream` until it ends or `signal` aborts. */
  async stream(handlers: StreamHandlers, signal?: AbortSignal): Promise<void> {
    const response = await this.get('/api/stream', signal);
    if (!response.body) return;
    const parser = new SseParser(({ event, data }) => {
      if (event === 'sample') handlers.sample(JSON.parse(data) as Sample);
      else if (event === 'state') handlers.state(JSON.parse(data) as StateChange);
    });
    const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return;
      parser.push(value);
    }
  }

  private async json<T>(path: string): Promise<T> {
    return (await (await this.get(path)).json()) as T;
  }

  private async get(path: string, signal?: AbortSignal): Promise<Response> {
    const response = await this.fetcher(`${this.base}${path}`, {
      method: 'GET',
      headers: { Authorization: `Bearer ${this.token}` },
      ...(signal ? { signal } : {}),
    });
    if (!response.ok) throw new Error(`GET ${path} answered ${response.status}`);
    return response;
  }
}
