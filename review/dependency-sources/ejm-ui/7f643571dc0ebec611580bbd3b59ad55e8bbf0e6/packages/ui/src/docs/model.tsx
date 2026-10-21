import type { CalloutKind } from '@ejm/core';
import type { ReactNode } from 'react';
import { Callout } from './Callout';
import { CodeBlock } from './CodeBlock';
import { CodeTabs, type CodeTab } from './CodeTabs';
import type { ContentsItem } from './Contents';
import type { DocsNavGroup } from './DocsSidebar';
import { Heading } from './Heading';
import type { PagerLink } from './Pager';
import { ParamTable, type Param } from './ParamTable';
import { DocTable } from './DocTable';
import { Prose } from './Prose';

type ParamLabels = NonNullable<Parameters<typeof ParamTable>[0]['labels']>;

/**
 * A documentation set as plain data, with no MDX and no build step. A page is a title, a lead and sections. A section is
 * an h2 with blocks, and a block may be an h3. The sidebar, the contents list and the previous and next links are all
 * derived from this data, so they can never disagree with the page.
 */
export type Block =
  | { type: 'p'; children: ReactNode }
  | { type: 'list'; items: readonly ReactNode[]; ordered?: boolean; start?: number }
  | { type: 'code'; code: string; lang?: string; label?: string }
  | { type: 'tabs'; label?: string; tabs: readonly CodeTab[] }
  | { type: 'callout'; kind: CalloutKind; children: ReactNode }
  | { type: 'params'; caption: string; rows: readonly Param[]; labels?: ParamLabels }
  | { type: 'h3'; id: string; title: string }
  | { type: 'table'; caption: string; heads: readonly ReactNode[]; rows: readonly (readonly ReactNode[])[] }
  | { type: 'node'; children: ReactNode };

export interface DocSection {
  id: string;
  title: string;
  blocks: readonly Block[];
}

export interface DocPage {
  /** The last path segment. */
  slug: string;
  /** The sidebar group the page sits in. */
  group: string;
  title: string;
  /** One or two sentences under the title; also the page description. */
  lead: string;
  sections: readonly DocSection[];
}

export interface DocSet {
  /** Where the set lives, for example "/docs". The first page is the index; the others are `${base}/${slug}`. */
  base: string;
  pages: readonly DocPage[];
}

export const pageHref = (set: DocSet, page: DocPage) => (page === set.pages[0] ? set.base : `${set.base}/${page.slug}`);

export function navGroups(set: DocSet): DocsNavGroup[] {
  const groups: DocsNavGroup[] = [];
  for (const page of set.pages) {
    let g = groups.find((x) => x.title === page.group);
    if (!g) groups.push((g = { title: page.group, items: [] }));
    (g.items as { label: string; href: string }[]).push({ label: page.title, href: pageHref(set, page) });
  }
  return groups;
}

export function pageContents(page: DocPage): ContentsItem[] {
  return page.sections.flatMap((s) => [
    { id: s.id, label: s.title, level: 2 as const },
    ...s.blocks.flatMap((b) => (b.type === 'h3' ? [{ id: b.id, label: b.title, level: 3 as const }] : [])),
  ]);
}

export function pagerFor(set: DocSet, page: DocPage): { prev: PagerLink | null; next: PagerLink | null } {
  const i = set.pages.indexOf(page);
  const link = (p: DocPage | undefined) => (p ? { href: pageHref(set, p), title: p.title } : null);
  return { prev: link(set.pages[i - 1]), next: link(set.pages[i + 1]) };
}

function BlockView({ block, compact }: { block: Block; compact?: boolean }) {
  switch (block.type) {
    case 'p':
      return compact ? <span>{block.children}</span> : <p>{block.children}</p>;
    case 'list': {
      const List = block.ordered ? 'ol' : 'ul';
      return (
        <List start={block.ordered ? block.start : undefined} className={block.ordered ? 'list-decimal ps-6' : 'list-disc ps-6'}>
          {block.items.map((item, i) => (
            <li key={i} className="mt-2 first:mt-0">
              {item}
            </li>
          ))}
        </List>
      );
    }
    case 'code':
      return <CodeBlock code={block.code} lang={block.lang} label={block.label} />;
    case 'tabs':
      return <CodeTabs label={block.label} tabs={block.tabs} />;
    case 'callout':
      return <Callout kind={block.kind}>{block.children}</Callout>;
    case 'table':
      return <DocTable caption={block.caption} heads={block.heads} rows={block.rows} />;
    case 'params':
      return <ParamTable caption={block.caption} rows={block.rows} labels={block.labels} />;
    case 'h3':
      return <Heading level={3} id={block.id}>{block.title}</Heading>;
    case 'node':
      return <>{block.children}</>;
  }
}

/** Blocks from the React content model. Compact keeps a list item's first paragraph inline. */
export function DocBlocks({ blocks, compact = false }: { blocks: readonly Block[]; compact?: boolean }) {
  return <>{blocks.map((block, i) => <BlockView key={i} block={block} compact={compact && i === 0} />)}</>;
}

/** The reading column: the page's sections as h2 with their blocks. The h1 and lead come from `DocHead`. */
export function DocBody({ page }: { page: DocPage }) {
  return (
    <Prose>
      {page.sections.map((s) => (
        <section key={s.id} aria-labelledby={s.id} className="mt-20 first:mt-0">
          <Heading id={s.id}>{s.title}</Heading>
          <DocBlocks blocks={s.blocks} />
        </section>
      ))}
    </Prose>
  );
}

/** The single h1 and the lead of a page. */
export function DocHead({ page }: { page: DocPage }) {
  return (
    <header>
      <p className="text-meta text-muted">{page.group}</p>
      <h1 className="mt-3 max-w-[18ch] text-title">{page.title}</h1>
      <p className="mt-6 max-w-measure text-lead text-muted">{page.lead}</p>
    </header>
  );
}
