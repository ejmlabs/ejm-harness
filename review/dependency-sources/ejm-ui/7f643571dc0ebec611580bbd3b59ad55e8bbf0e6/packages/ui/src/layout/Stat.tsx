import { contract, type StatState } from '@ejm/core';
import type { ReactNode } from 'react';
import { cn } from '../lib/utils';
import { Status } from './Status';

export interface StatProps {
  /** The figure, with its unit: "100", "4.2 ms". Large numerals in tabular figures. */
  figure?: ReactNode;
  /** The mono label under the figure. */
  label: string;
  state?: StatState;
  /** Figure scale. Use title for dense examples or narrow compositions. */
  size?: 'stat' | 'title';
  /** Replaces the default word for loading, error or empty. */
  message?: string;
  className?: string;
}

/** A large figure with a mono label and a hairline above. No box. Loading, error and empty replace the figure with a word. */
export function Stat({ figure, label, state = 'default', size = 'stat', message, className }: StatProps) {
  const words = contract.text.stat;
  return (
    <div className={cn('hairline-t pt-6', className)} data-state={state} aria-busy={state === 'loading' || undefined}>
      {state === 'default' ? (
        <p className={cn(size === 'title' ? 'text-title' : 'text-stat', '-ms-[0.05em] tabular-nums')}>{figure}</p>
      ) : state === 'error' ? (
        <p className="text-heading" role="status"><Status status="error" label={message ?? words.error} className="text-heading" /></p>
      ) : (
        <p className="text-heading text-muted" role={state === 'loading' ? 'status' : undefined}>{message ?? words[state]}</p>
      )}
      <p className="mt-3 text-meta text-muted">{label}</p>
    </div>
  );
}
