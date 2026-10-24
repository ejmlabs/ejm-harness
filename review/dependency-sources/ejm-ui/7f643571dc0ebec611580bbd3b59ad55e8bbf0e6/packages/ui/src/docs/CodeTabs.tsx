import type { ReactNode } from 'react';
import { CodeBlock } from './CodeBlock';
import { CodeTabsFrame } from './CodeTabsFrame';
import { useHydrated } from './hooks';

export interface CodeTab {
  label: string;
  code?: string;
  lang?: string;
  children?: ReactNode;
}

/** Language tabs with highlighted code, or caller-supplied content. */
export function CodeTabs({ label, tabs, className }: { label?: string; tabs: readonly CodeTab[]; className?: string }) {
  const hydrated = useHydrated();
  return <CodeTabsFrame label={label} className={className} tabs={tabs.map((tab) => ({
    label: tab.label,
    children: tab.code !== undefined ? <CodeBlock code={tab.code} lang={tab.lang} quietLabel={hydrated} className={hydrated ? 'my-0 border-t-0' : 'mb-0 mt-2'} /> : tab.children,
  }))} />;
}
