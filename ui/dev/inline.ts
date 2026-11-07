// `pnpm build:panel` only: folds the emitted script chunk and stylesheet into the page that loads
// them, so the panel is one HTML file that loads nothing else, as `GET /__spindle/ui` answers it.

import type { Plugin, Rolldown } from 'vite';

type OutputAsset = Rolldown.OutputAsset;
type OutputChunk = Rolldown.OutputChunk;

/**
 * What the HTML tokenizer's script data states read inside the inlined script: `</script` ends the
 * element, and `<!--` followed by `<script` hides the next `</script` from it. The script is a
 * module, where `<!--` is no comment and `<script` is no code, so both occur only inside a string,
 * a template, a regular expression or a comment. There `<` is written `\x3C`, which each of those
 * reads as the same character (a comment does not read it at all). After an odd run of backslashes
 * the `<` is already escaped, and only `x3C` is put in its place.
 */
const scriptText = (code: string): string =>
  code
    .replace(/<\/(script)/gi, '<\\/$1')
    .replace(/(\\*)<(?=!--|script)/gi, (_run, slashes: string) =>
      slashes.length % 2 === 1 ? `${slashes}x3C` : `${slashes}\\x3C`,
    );

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
