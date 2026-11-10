import { defineConfig } from '@playwright/test';
import { join } from 'node:path';
import { build, port } from './adapter.ts';
import { OUT } from './e2e/out.ts';

export default defineConfig({
  testDir: './e2e',
  globalSetup: './e2e/setup.ts',
  outputDir: join(OUT, '..', 'test-results'),
  fullyParallel: true,
  workers: Number(process.env.UX_WORKERS ?? 6),
  timeout: 180_000,
  reporter: [['list'], ['json', { outputFile: join(OUT, '..', 'playwright.json') }]],
  use: {
    browserName: 'chromium',
    headless: true,
    deviceScaleFactor: 1,
    locale: 'en-US',
    timezoneId: 'UTC',
  },
  projects: [
    { name: 'gating', grepInvert: /@measured/ },
    { name: 'measured', grep: /@measured/ },
  ],
  webServer: {
    command: `node harness/server.ts ${port} ${build}`,
    url: `http://127.0.0.1:${port}/__ux/health`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
