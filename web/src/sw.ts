/// <reference lib="webworker" />

// The Service Worker: opens the wasm host once, with the specification, environment and
// decisions file `spindle.json` carries, and answers every request the host says it serves
// (`Spindle.serves`: the worker's own origin, a route of the specification or a console route).
// Every other request goes to the network as it would without a worker. This file decides
// nothing itself: it only hands requests to the host and its answers to `respondWith`.

import init, { Spindle } from './wasm/spindle_wasm.js';
import wasmUrl from './wasm/spindle_wasm_bg.wasm?url';

declare const self: ServiceWorkerGlobalScope;

interface Bundle {
  sources: [string, string][];
  environment: string;
  decisions: string | null;
  binding: string | null;
}

let opened: Spindle | undefined;
let opening: Promise<Spindle> | undefined;

// The host, opened on the first call; an open that fails is forgotten, so the next call tries
// again rather than answering every later request with the same failure.
function spindle(): Promise<Spindle> {
  opening ??= (async () => {
    await init({ module_or_path: wasmUrl });
    const scope = new URL(self.registration.scope);
    const response = await fetch(new URL('spindle.json', scope));
    if (!response.ok) {
      throw new Error(`spindle.json answered ${response.status}`);
    }
    const bundle = (await response.json()) as Bundle;
    opened = await Spindle.open(
      JSON.stringify({ ...bundle, database: `spindle:${scope.pathname}`, base: scope.pathname }),
    );
    return opened;
  })().catch((error: unknown) => {
    opening = undefined;
    throw error;
  });
  return opening;
}

function answer(host: Spindle, request: Request): Promise<Response> {
  return host.fetch(request) as Promise<Response>;
}

self.addEventListener('install', (event) => {
  event.waitUntil(spindle().then(() => self.skipWaiting()));
});

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (opened === undefined) {
    // A restarted worker has not opened the host yet: the host decides once it is open, and a
    // request it does not serve, or one that arrives while the open fails, goes to the network.
    event.respondWith(
      spindle().then(
        (host) => (host.serves(request.url, request.method) ? answer(host, request) : fetch(request)),
        (error: unknown) => {
          console.error('the Spindle host did not open', error);
          return fetch(request);
        },
      ),
    );
  } else if (opened.serves(request.url, request.method)) {
    event.respondWith(answer(opened, request));
  }
});
