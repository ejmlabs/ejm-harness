import type { CalloutKind } from '@ejm/core';
import { glyphs } from '@ejm/core/contract.json';
import type { ReactNode } from 'react';
import { cn } from '../lib/utils';

const rule: Record<CalloutKind, string> = {
  note: 'border-info',
  tip: 'border-success',
  warning: 'border-warn',
};
const tone: Record<CalloutKind, string> = {
  note: 'text-info',
  tip: 'text-success',
  warning: 'text-warn',
};

/**
 * A short aside the reader must not miss: a left rule and a text label first (Note, Tip or Warning). No box and no fill.
 * The word carries the meaning; the rule color only backs it up. On a Sage band every status color is Ink, so the word still reads.
 */
export function Callout({ kind = 'note', children, className }: { kind?: CalloutKind; children: ReactNode; className?: string }) {
  const label = glyphs.callout[kind].label;
  return (
    <div role="note" data-kind={kind} className={cn('my-8 border-s-2 ps-5', rule[kind], className)}>
      <p className={cn('text-meta font-medium uppercase tracking-wider', tone[kind])}>{label}</p>
      <div className="mt-2 max-w-measure space-y-3 text-fg [&>:first-child]:mt-0">{children}</div>
    </div>
  );
}
