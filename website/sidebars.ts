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
        'concepts/environment-and-admission',
        'concepts/components-and-bindings',
        'concepts/deployment-targets',
        'concepts/observe',
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
