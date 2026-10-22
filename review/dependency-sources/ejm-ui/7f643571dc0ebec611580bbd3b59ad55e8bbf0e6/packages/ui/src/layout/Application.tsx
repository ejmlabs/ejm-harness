import { createElement, type FieldsetHTMLAttributes, type FormHTMLAttributes, type HTMLAttributes, type Ref } from 'react';

type ContentAttributes<T extends HTMLElement = HTMLElement> = Omit<HTMLAttributes<T>, 'className' | 'style' | 'dangerouslySetInnerHTML'>;
type ContentTag = 'div' | 'section' | 'article' | 'aside' | 'header' | 'nav' | 'blockquote' | 'ul' | 'ol' | 'li' | 'dl';
export type AppContentProps = ContentAttributes & { as?: ContentTag; ref?: Ref<HTMLElement> };

/** Application composition accepts content and semantics. The shared owner fixes its visual rules. */
export function AppFlow({ as = 'div', ref, ...props }: AppContentProps) {
  return createElement(as, { ...props, ref, className: 'grid min-w-0 grid-cols-1 gap-8 [overflow-wrap:anywhere]' });
}

export function AppGroup({ as = 'div', ref, ...props }: AppContentProps) {
  return createElement(as, { ...props, ref, className: 'grid min-w-0 grid-cols-1 gap-4 [overflow-wrap:anywhere]' });
}

export function AppActions({ as = 'div', ref, ...props }: AppContentProps) {
  return createElement(as, { ...props, ref, className: 'flex min-w-0 flex-wrap items-center gap-3' });
}

export function AppHeading({ as: H = 'h2', ref, ...props }: ContentAttributes<HTMLHeadingElement> & { as?: 'h2' | 'h3' | 'h4'; ref?: Ref<HTMLHeadingElement> }) {
  return <H {...props} ref={ref} className="text-heading" />;
}

export function AppField({ ref, ...props }: ContentAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return <div {...props} ref={ref} className="grid min-w-0 grid-cols-1 gap-2 [&>label]:text-meta [&>label]:font-medium" />;
}

export function AppFieldset({ ref, ...props }: Omit<FieldsetHTMLAttributes<HTMLFieldSetElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'> & { ref?: Ref<HTMLFieldSetElement> }) {
  return <fieldset {...props} ref={ref} className="grid min-w-0 grid-cols-1 gap-4 [&>legend]:mb-4 [&>legend]:text-meta [&>legend]:font-medium" />;
}

export function AppForm({ ref, ...props }: Omit<FormHTMLAttributes<HTMLFormElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'> & { ref?: Ref<HTMLFormElement> }) {
  return <form {...props} ref={ref} className="grid min-w-0 max-w-xl grid-cols-1 gap-6" />;
}

/** Account and information pages use the same fixed reading measure. */
export function AppSection({ ref, ...props }: ContentAttributes<HTMLElement> & { ref?: Ref<HTMLElement> }) {
  return <section {...props} ref={ref} className="grid min-w-0 max-w-3xl grid-cols-1 gap-8 [overflow-wrap:anywhere]" />;
}

export function AppPre({ ref, ...props }: ContentAttributes<HTMLPreElement> & { ref?: Ref<HTMLPreElement> }) {
  return <pre {...props} ref={ref} className="min-w-0 max-w-full whitespace-pre-wrap font-mono text-meta [overflow-wrap:anywhere]" />;
}

export { GameWorkspace, PlayerStrip, BoardPlaceholder, MovePairs, MoveHistory, type GameWorkspaceProps, type MoveHistoryProps } from './ApplicationGame';
