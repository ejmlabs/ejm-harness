import { ChevronDown } from 'lucide-react';
import type { ComponentProps, ReactNode } from 'react';
import { cn } from '../../lib/utils';

export interface DisclosureProps extends ComponentProps<'details'> {
  summary: ReactNode;
}

/** A native disclosure at every width. Closed content stays mounted, so forms retain input and widget state. */
export function Disclosure({ summary, children, className, ...props }: DisclosureProps) {
  return (
    <details className={cn('group border-t border-line', className)} {...props}>
      <summary className="touch-target focus-ring flex min-h-11 cursor-pointer list-none items-center justify-between gap-4 py-2 text-fg marker:hidden hover:text-link [&::-webkit-details-marker]:hidden">
        <span className="min-w-0 font-semibold">{summary}</span>
        <ChevronDown aria-hidden className="size-4 shrink-0 transition-transform duration-base ease-standard group-open:rotate-180" />
      </summary>
      <div className="pb-4 pt-2">{children}</div>
    </details>
  );
}
