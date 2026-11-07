/// <reference lib="webworker" />

// The Service Worker: opens the wasm host once, with the specification, environment and
// decisions file `spindle.json` carries, and answers every request whose path is one of the
// specification's routes or a console route. Every other request goes to the network as it would
// without a worker.

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
  })();
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
  if (new URL(request.url).origin !== self.location.origin) {
    return;
  }
  if (opened !== undefined) {
    if (opened.serves(request)) {
      event.respondWith(answer(opened, request));
    }
    return;
  }
  event.respondWith(
    spindle().then((host) => (host.serves(request) ? answer(host, request) : fetch(request))),
  );
});
