import { Fragment, type ComponentType, type ReactNode } from 'react';
import { DocsLayout } from '../shell/DocsLayout';
import { Contents } from './Contents';
import type { ClientDocPage, DocCode, DocDataBlock } from './data';
import { CodeBlockFrame, CodeSource } from './CodeBlockFrame';
import { DocCodeBlock } from './DocCodeBlock';
import { DocsSidebar, type DocsNavGroup } from './DocsSidebar';
import { languageName } from './language';
import { DocHead } from './model';
import { Pager, type PagerLink } from './Pager';
import { Prose } from './Prose';

export interface DocNavigationPage { href: string; title: string; group: string; label?: string }
/** Keep the consumer's final route paths intact, including its trailing slash policy. */
export function docNavigation(pages: readonly DocNavigationPage[], current: string) {
  const at = pages.findIndex((page) => page.href === current);
  if (at < 0) throw new Error(`Unknown docs page: ${current}`);
  const groups: DocsNavGroup[] = [];
  for (const page of pages) {
    let group = groups.find((item) => item.title === page.group);
    if (!group) groups.push(group = { title: page.group, items: [] });
    (group.items as { href: string; label: string }[]).push({ href: page.href, label: page.label ?? page.title });
  }
  const link = (page?: DocNavigationPage): PagerLink | null => page ? { href: page.href, title: page.label ?? page.title } : null;
  return { groups, current, prev: link(pages[at - 1]), next: link(pages[at + 1]) };
}

export interface DocsPageProps {
  page: ClientDocPage;
  groups: readonly DocsNavGroup[];
  current: string;
  prev?: PagerLink | null;
  next?: PagerLink | null;
  filter?: boolean;
  sidebarLabel?: string;
  /** Supply @ejm/ssg Static to retain prose in prerendered HTML. Without it, renderProse renders normally. */
  staticBoundary?: ComponentType<{ render?: () => ReactNode }>;
  /** Set only in SSR or dev. Its source data can then be removed from a production browser build. */
  renderProse?: (section: number, item: number) => ReactNode;
  /** Server/dev-only highlighted source. Omit it for plain code. Its copy figure remains live. */
  renderCode?: (block: DocCode) => ReactNode;
  /** Required when splitDoc reports requiresRich, for code nested in lists or callouts. */
  renderBlock?: (block: DocDataBlock, renderCode: (code: DocCode) => ReactNode) => ReactNode;
}
const Live = ({ render }: { render?: () => ReactNode }) => <>{render?.()}</>;

/** A live docs shell around static prose and live code, tabs, contents, and navigation. */
export function DocsPage({ page, groups, current, prev = null, next = null, filter, sidebarLabel, staticBoundary: Boundary = Live, renderProse, renderCode, renderBlock }: DocsPageProps) {
  const renderPlainCode = (import.meta as { env?: { SSR?: boolean; DEV?: boolean } }).env?.SSR
    || (import.meta as { env?: { SSR?: boolean; DEV?: boolean } }).env?.DEV
    || typeof window === 'undefined';
  const code = (block: DocCode) => (
    <CodeBlockFrame code={block.code} lang={block.lang} label={block.label}>
      {Boundary === Live && !renderCode ? undefined : <Boundary render={renderCode
        ? () => renderCode(block)
        : renderPlainCode
          ? () => <CodeSource code={block.code.replace(/\n$/, '')} label={block.label ?? languageName(block.lang ?? 'text')} />
          : undefined} />}
    </CodeBlockFrame>
  );
  const live = (block: DocDataBlock) => {
    if (block.type === 'code' || block.type === 'tabs') return <DocCodeBlock block={block} renderCode={code} />;
    if (!renderBlock) throw new Error('DocsPage needs renderBlock for nested interactive content. Check splitDoc.requiresRich at build time.');
    return renderBlock(block, code);
  };
  return <DocsLayout sidebar={<DocsSidebar groups={groups} current={current} filter={filter} label={sidebarLabel} />} contents={<Contents items={page.contents} />}>
    <article>
      <DocHead page={{ ...page, sections: [] }} />
      <div className="mt-16"><Prose>
        {page.sections.map((section, si) => <section key={section.id} aria-labelledby={section.id} className="mt-20 first:mt-0">
          {section.items.map((item, ii) => item.type === 'prose'
            ? <Boundary key={ii} render={renderProse ? () => renderProse(si, ii) : undefined} />
            : <Fragment key={ii}>{live(item)}</Fragment>)}
        </section>)}
      </Prose></div>
      <Pager prev={prev} next={next} />
    </article>
  </DocsLayout>;
}
