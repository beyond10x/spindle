import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { inline } from './dev/inline.ts';

// The observe panel a served run answers at `GET /__spindle/ui`: `panel.html` with its script and
// style inlined, out to `dist-panel/`, which `b10x-spindle` embeds.
export default defineConfig({
  base: './',
  plugins: [vue(), inline()],
  build: {
    outDir: 'dist-panel',
    emptyOutDir: true,
    modulePreload: false,
    cssCodeSplit: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    rolldownOptions: {
      input: 'panel.html',
      output: {
        codeSplitting: false,
      },
    },
  },
});
