import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    'index',
    'getting-started',
    {
      type: 'category',
      label: 'Concepts',
      collapsed: false,
      items: [
        'concepts/specification',
        'concepts/capabilities',
        {
          type: 'html',
          value: '<a class="menu__link" href="/spindle/docs/concepts/capabilities#serving-ingressnetwork-over-http">Serving ingress.network over HTTP</a>',
          defaultStyle: true,
        },
        'concepts/environment-and-admission',
        'concepts/components-and-bindings',
        'concepts/deployment-targets',
        'concepts/observe',
        {
          type: 'html',
          value: '<a class="menu__link" href="/spindle/docs/reference/cli#run--observe">run --observe</a>',
          defaultStyle: true,
        },
      ],
    },
    {
      type: 'category',
      label: 'Examples',
      collapsed: false,
      items: ['examples/rally'],
    },
    {
      type: 'category',
      label: 'Reference',
      collapsed: false,
      items: ['reference/cli'],
    },
    'status',
  ],
};

export default sidebars;
