// Serves the built site (`website/build`) under its base URL on 127.0.0.1, as a static host does:
// the exact file, else `<path>.html`, else `<path>/index.html`, else `404.html` with status 404.
// Hermetic: nothing but the build directory is read.
//
//   node ux/harness/server.ts <port> [<build dir>]

import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';

const port = Number(process.argv[2] ?? 4320);
const root = resolve(process.argv[3] ?? join(import.meta.dirname, '..', '..', 'build'));
const base = process.env.UX_BASE ?? '/spindle/';

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.xml': 'application/xml',
  '.txt': 'text/plain; charset=utf-8',
  '.ico': 'image/x-icon',
};

const file = (path: string): string | null => (existsSync(path) && statSync(path).isFile() ? path : null);

export function resolvePath(urlPath: string): string | null {
  if (!urlPath.startsWith(base)) return null;
  const rest = normalize(decodeURIComponent(urlPath.slice(base.length))).replace(/^(\.\.(\/|$))+/, '');
  const at = join(root, rest);
  if (!at.startsWith(root)) return null;
  if (rest === '' || rest === '.' || rest.endsWith('/')) return file(join(at, 'index.html'));
  return file(at) ?? file(`${at}.html`) ?? file(join(at, 'index.html'));
}

if (!existsSync(join(root, 'index.html'))) {
  console.error(`server: no built site at ${root}; build it first (npm --prefix website run build)`);
  process.exit(1);
}

createServer((req, res) => {
  const url = new URL(req.url ?? '/', `http://127.0.0.1:${port}`);
  if (url.pathname === '/__ux/health') {
    res.writeHead(200, { 'content-type': 'text/plain' }).end('ok');
    return;
  }
  const found = resolvePath(url.pathname);
  const path = found ?? join(root, '404.html');
  res.writeHead(found ? 200 : 404, { 'content-type': TYPES[extname(path)] ?? 'application/octet-stream', 'cache-control': 'no-store' });
  createReadStream(path).pipe(res);
}).listen(port, '127.0.0.1', () => console.log(`server: ${root} at http://127.0.0.1:${port}${base}`));
