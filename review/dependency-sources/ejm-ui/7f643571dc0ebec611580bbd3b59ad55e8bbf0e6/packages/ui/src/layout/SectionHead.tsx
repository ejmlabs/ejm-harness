import type { ReactNode } from 'react';
import { cn } from '../lib/utils';

export interface SectionHeadProps {
  /** Mono kicker, for example "01 / Projects". */
  kicker?: string;
  title: ReactNode;
  children?: ReactNode;
  /** The content's role selects the complete shared recipe. Heading level stays independent. */
  purpose?: 'page' | 'section' | 'application';
  as?: 'h1' | 'h2' | 'h3';
  id?: string;
  /** Legacy component styling. Authored sites use purpose and the shared recipe. */
  size?: 'display' | 'title';
  className?: string;
}

/** One opening recipe per content role. Sites provide words and heading semantics. */
export function SectionHead({ kicker, title, children, purpose = 'section', as: H = 'h2', id, size, className }: SectionHeadProps) {
  const application = purpose === 'application';
  const heading = <H id={id} className={(purpose === 'page' || size === 'display') ? 'text-display @3xl:col-span-7' : application ? 'text-title' : 'text-title @3xl:col-span-7'}>{title}</H>;
  const copy = children && <div className={application ? 'grid max-w-measure gap-6 text-body' : 'grid max-w-measure gap-6 text-lead @3xl:col-span-4 @3xl:col-start-9 @3xl:self-end'}>{children}</div>;
  return (
    <header className={cn(application ? '@container grid gap-field' : '@container', className)}>
      {kicker && <p className="text-meta text-muted">{kicker}</p>}
      {application ? <>{heading}{copy}</> :
        <div className={kicker ? 'grid gap-x-12 gap-y-8 @3xl:grid-cols-12 mt-6' : 'grid gap-x-12 gap-y-8 @3xl:grid-cols-12'}>{heading}{copy}</div>}
    </header>
  );
}
