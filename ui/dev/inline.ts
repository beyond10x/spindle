// `pnpm build:panel` only: folds the emitted script chunk and stylesheet into the page that loads
// them, so the panel is one HTML file that loads nothing else, as `GET /__spindle/ui` answers it.

import type { Plugin, Rolldown } from 'vite';

type OutputAsset = Rolldown.OutputAsset;
type OutputChunk = Rolldown.OutputChunk;

/** `</script` inside the inlined script would end its element early. */
const scriptText = (code: string): string => code.replace(/<\/(script)/gi, '<\\/$1');

/** `</style` inside the inlined stylesheet would end its element early. */
const styleText = (code: string): string => code.replace(/<\/(style)/gi, '<\\/$1');

export function inline(): Plugin {
  return {
    name: 'spindle-inline-panel',
    apply: 'build',
    enforce: 'post',
    generateBundle(_options, bundle) {
      const pages = Object.values(bundle).filter(
        (output): output is OutputAsset => output.type === 'asset' && output.fileName.endsWith('.html'),
      );
      const inlined = new Set<string>();
      for (const page of pages) {
        let html = String(page.source);
        html = html.replace(
          /<script\b[^>]*\bsrc="\.?\/?([^"]+)"[^>]*><\/script>/g,
          (tag, name: string) => {
            const chunk = bundle[name];
            if (!chunk || chunk.type !== 'chunk') return tag;
            inlined.add(name);
            return `<script type="module">${scriptText((chunk as OutputChunk).code)}</script>`;
          },
        );
        html = html.replace(/<link\b[^>]*\bhref="\.?\/?([^"]+)"[^>]*>/g, (tag, name: string) => {
          const asset = bundle[name];
          if (!asset || asset.type !== 'asset' || !name.endsWith('.css')) return tag;
          inlined.add(name);
          return `<style>${styleText(String(asset.source))}</style>`;
        });
        page.source = html;
      }
      for (const name of inlined) delete bundle[name];
    },
  };
}
