import type { ReactNode } from 'react';
import { ExternalLink } from '../shell/ExternalLink';
import { useLinkComponent } from '../shell/link';
import { DocCodeBlock } from './DocCodeBlock';
import type { DocCode, DocDataBlock as DataBlock, DocInline } from './data';
import { Heading } from './Heading';
import { Callout } from './Callout';
import { DocTable } from './DocTable';

function Inline({ nodes }: { nodes: readonly DocInline[] }) {
  const Link = useLinkComponent();
  return <>{nodes.map((node, i): ReactNode => {
    if (typeof node === 'string') return node;
    switch (node[0]) {
      case 'code': return <code key={i}>{node[1]}</code>;
      case 'strong': return <strong key={i}><Inline nodes={node[1]} /></strong>;
      case 'em': return <em key={i}><Inline nodes={node[1]} /></em>;
      case 'del': return <del key={i}><Inline nodes={node[1]} /></del>;
      case 'br': return <br key={i} />;
      case 'a': return node[2]
        ? <ExternalLink key={i} href={node[1]}><Inline nodes={node[3]} /></ExternalLink>
        : <Link key={i} href={node[1]}><Inline nodes={node[3]} /></Link>;
    }
  })}</>;
}

/** Render serialized blocks through the shared docs parts, including nested code. */
export function DocDataBlock({ block, compact = false, renderCode }: { block: DataBlock; compact?: boolean; renderCode?: (code: DocCode) => ReactNode }): ReactNode {
  switch (block.type) {
    case 'p': return compact ? <span><Inline nodes={block.children} /></span> : <p><Inline nodes={block.children} /></p>;
    case 'list': {
      const List = block.ordered ? 'ol' : 'ul';
      return <List start={block.ordered ? block.start : undefined} className={block.ordered ? 'list-decimal ps-6' : 'list-disc ps-6'}>
        {block.items.map((item, i) => <li key={i} className="mt-2 first:mt-0">{item.map((child, j) => <DocDataBlock key={j} block={child} compact={j === 0} renderCode={renderCode} />)}</li>)}
      </List>;
    }
    case 'code':
    case 'tabs': return <DocCodeBlock block={block} renderCode={renderCode} />;
    case 'h3': return <Heading level={3} id={block.id}>{block.title}</Heading>;
    case 'callout': return <Callout kind={block.kind}>{block.blocks.map((child, i) => <DocDataBlock key={i} block={child} renderCode={renderCode} />)}</Callout>;
    case 'table': return <DocTable caption={block.caption} heads={block.heads} rows={block.rows.map((row) => row.map((cell, i) => <Inline key={i} nodes={cell} />))} />;
  }
}
