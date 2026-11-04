import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import {withProductSite} from '@beyond10x/docs-system/product-site';

const config: Config = {
  title: 'Spindle',
  tagline: 'Run ESS specifications on the capabilities an environment provides.',
  favicon: 'img/favicon.svg',

  future: {
    v4: true,
  },

  url: 'https://beyond10x.github.io',
  baseUrl: '/spindle/',

  organizationName: 'beyond10x',
  projectName: 'spindle',
  deploymentBranch: 'gh-pages',
  trailingSlash: false,

  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',

  markdown: {
    format: 'detect',
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
    mermaid: true,
  },

  themes: ['@docusaurus/theme-mermaid'],

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          routeBasePath: 'docs',
          editUrl: 'https://github.com/beyond10x/spindle/tree/main/website/',
        },
        blog: false,
        theme: {customCss: './src/css/custom.css'},
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    navbar: {
      title: 'Spindle',
      logo: {alt: 'Spindle', src: 'img/logo.svg', width: 32, height: 32},
      items: [
        {to: '/docs/', label: 'Documentation', position: 'left'},
        {to: '/docs/getting-started', label: 'Getting started', position: 'left'},
        {to: '/docs/examples/rally', label: 'Rally', position: 'left'},
        {to: '/docs/reference/cli', label: 'CLI', position: 'left'},
        {to: '/docs/status', label: 'Status', position: 'left'},
        {href: 'https://github.com/beyond10x/spindle', label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      links: [
        {
          title: 'Documentation',
          items: [
            {label: 'Overview', to: '/docs/'},
            {label: 'Getting started', to: '/docs/getting-started'},
            {label: 'The specification', to: '/docs/concepts/specification'},
            {label: 'Capabilities', to: '/docs/concepts/capabilities'},
            {label: 'Environment and admission', to: '/docs/concepts/environment-and-admission'},
            {label: 'Components and bindings', to: '/docs/concepts/components-and-bindings'},
            {label: 'Deployment targets', to: '/docs/concepts/deployment-targets'},
            {label: 'Rally', to: '/docs/examples/rally'},
            {label: 'CLI', to: '/docs/reference/cli'},
            {label: 'Status', to: '/docs/status'},
          ],
        },
        {
          title: 'Project',
          items: [
            {label: 'Source', href: 'https://github.com/beyond10x/spindle'},
            {label: 'Changelog', href: 'https://github.com/beyond10x/spindle/blob/main/CHANGELOG.md'},
            {label: 'Design', href: 'https://github.com/beyond10x/spindle/blob/main/docs/design/spindle.md'},
            {label: 'ESS', href: 'https://beyond10x.github.io/ess/'},
          ],
        },
      ],
      copyright: 'A beyond10x project. Spindle · Apache-2.0.',
    },
  } satisfies Preset.ThemeConfig,
};

export default withProductSite(config, {landing: './product.json', mark: 'Sp'});
