// Registers the id of each anchored table row (`src/rehype/option-anchors.ts`) with Docusaurus's
// broken-link check, so a Markdown link may land on a row: `cli.md#run--observe-requests`.
import React, {type ComponentProps} from 'react';
import MDXComponents from '@theme-original/MDXComponents';
import useBrokenLinks from '@docusaurus/useBrokenLinks';

function Row(props: ComponentProps<'tr'>): React.JSX.Element {
  const brokenLinks = useBrokenLinks();
  if (props.id) brokenLinks.collectAnchor(props.id);
  return <tr {...props} />;
}

export default {...MDXComponents, tr: Row};
