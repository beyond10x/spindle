import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';
import { fixturesApi } from './dev/fixtures-api.ts';

export default defineConfig({
  base: './',
  plugins: [vue(), fixturesApi()],
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
  test: {
    environment: 'happy-dom',
    include: ['src/**/*.test.ts'],
  },
});
