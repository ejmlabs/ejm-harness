import { CodeBlock } from './CodeBlock';
import type { DocDataBlock as DataBlock } from './data';
import { DocDataBlock } from './DocDataBlock';
import { Heading } from './Heading';

/** A server-rendered prose run. The first run includes the section heading. */
export function DocProse({ section, blocks, first = false }: { section: { id: string; title: string }; blocks: readonly DataBlock[]; first?: boolean }) {
  return <>{first && <Heading id={section.id}>{section.title}</Heading>}{blocks.map((block, i) => <DocDataBlock key={i} block={block} renderCode={(code) => <CodeBlock code={code.code} lang={code.lang} label={code.label} />} />)}</>;
}
