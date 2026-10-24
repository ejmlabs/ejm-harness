import { lazy, Suspense, type ReactNode } from 'react';
import { CodeBlockFrame } from './CodeBlockFrame';
import type { DocCode, DocCodeTabs } from './data';

// Resolve the server frame before synchronous prerendering. Browsers load it only for a tab block.
const server = (import.meta as { env?: { SSR?: boolean } }).env?.SSR ?? (typeof window === 'undefined');
const CodeTabsFrame = server
  ? (await import('./CodeTabsFrame')).CodeTabsFrame
  : lazy(() => import('./CodeTabsFrame').then(({ CodeTabsFrame }) => ({ default: CodeTabsFrame })));

/** Live code controls without the prose renderer or syntax tokenizer. */
export function DocCodeBlock({ block, renderCode }: { block: DocCode | DocCodeTabs; renderCode?: (code: DocCode) => ReactNode }): ReactNode {
  if (block.type === 'code') return renderCode ? renderCode(block) : <CodeBlockFrame code={block.code} lang={block.lang} label={block.label} />;
  return <Suspense fallback={null}>
    <CodeTabsFrame label={block.label} tabs={block.tabs.map((tab) => ({ label: tab.label, children: <DocCodeBlock block={{ type: 'code', code: tab.code, lang: tab.lang, label: tab.codeLabel }} renderCode={renderCode} /> }))} />
  </Suspense>;
}
