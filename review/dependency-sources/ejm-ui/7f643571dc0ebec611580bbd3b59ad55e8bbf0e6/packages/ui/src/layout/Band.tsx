import type { Band as BandName } from '@ejm/tokens';
import type { HTMLAttributes } from 'react';
import { cn } from '../lib/utils';

export type BandTone = BandName;

export interface PageProps extends HTMLAttributes<HTMLDivElement> {
  /** Applications use 32px phone / 48px desktop padding and shared content gaps. The default is the column alone. */
  variant?: 'default' | 'application';
}

/** The content column. Bands stay full bleed; application pages add a compact content rhythm. */
export function Page({ variant = 'default', className, ...rest }: PageProps) {
  return <div className={cn('page', variant === 'application' && 'grid gap-content py-8 sm:py-12 animate-slide', className)} {...rest} />;
}

export interface BandProps extends HTMLAttributes<HTMLElement> {
  /** The ground: paper, mist, sage or ink. Every child reads correctly on every band. */
  tone?: BandTone;
  as?: 'section' | 'div' | 'header' | 'footer' | 'aside' | 'article' | 'main';
  /** Wrap children in the page column (default). Pass false to place your own `Page`. */
  column?: boolean;
  /** Drop the vertical rhythm, for a band that is only a ground (a header or a thin strip). */
  flush?: boolean;
}

/** A full-bleed section with its own ground and the vertical rhythm of the page. */
export function Band({ tone = 'paper', as: Tag = 'section', column = true, flush = false, className, children, ...rest }: BandProps) {
  return (
    <Tag data-band={tone} className={cn('band', !flush && 'py-rhythm', className)} {...rest}>
      {column ? <Page>{children}</Page> : children}
    </Tag>
  );
}
