import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import vue from '@vitejs/plugin-vue';

const here = import.meta.dirname;

/// What the Service Worker opens the host with, besides the database and the base it adds:
/// the shape of `spindle_wasm::Config`.
interface Bundle {
  sources: [string, string][];
  environment: string;
  decisions: string | null;
  binding: string | null;
}

function yamlFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.yaml'))
    .map((entry) => join(entry.parentPath, entry.name))
    .sort();
}

function optional(path: string | undefined): string | null {
  return path !== undefined && existsSync(path) ? readFileSync(path, 'utf8') : null;
}

/// Emits `spindle.json`: the specification's sources, the environment document, the decisions
/// file and the `ess-http/1` binding the Service Worker serves. Each is read from the path its
/// variable names: `SPINDLE_SPECIFICATION` (a directory with `system.yaml`),
/// `SPINDLE_ENVIRONMENT`, `SPINDLE_DECISIONS` and `SPINDLE_BINDING`.
function specification(): Plugin {
  const directory = resolve(
    here,
    process.env.SPINDLE_SPECIFICATION ?? '../crates/spindle-wasm/tests/fixtures/review',
  );
  const environment = resolve(
    here,
    process.env.SPINDLE_ENVIRONMENT ?? '../examples/environments/browser.toml',
  );
  const decisions = resolve(here, process.env.SPINDLE_DECISIONS ?? join(directory, 'decisions.toml'));
  const binding = process.env.SPINDLE_BINDING;
  return {
    name: 'spindle-specification',
    generateBundle() {
      const bundle: Bundle = {
        sources: yamlFiles(directory).map((file) => [
          relative(directory, file).split(sep).join('/'),
          readFileSync(file, 'utf8'),
        ]),
        environment: readFileSync(environment, 'utf8'),
        decisions: optional(decisions),
        binding: optional(binding === undefined ? undefined : resolve(here, binding)),
      };
      this.emitFile({
        type: 'asset',
        fileName: 'spindle.json',
        source: JSON.stringify(bundle),
      });
    },
  };
}

export default defineConfig({
  base: './',
  plugins: [vue(), specification()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    rolldownOptions: {
      input: {
        index: resolve(here, 'index.html'),
        sw: resolve(here, 'src/sw.ts'),
      },
      output: {
        entryFileNames: (chunk) => (chunk.name === 'sw' ? 'sw.js' : 'assets/[name]-[hash].js'),
      },
    },
  },
});
