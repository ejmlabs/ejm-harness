import type { ReactNode } from 'react';
import { cn } from '../lib/utils';

/** The shared reading surface for both React-authored and compiled documentation. */
export function Prose({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('text-body [&_:not(pre)>code]:rounded-control [&_:not(pre)>code]:bg-tone [&_:not(pre)>code]:px-1.5 [&_:not(pre)>code]:py-0.5 [&_:not(pre)>code]:font-mono [&_:not(pre)>code]:text-[0.875em] [&_:not(pre)>code]:[overflow-wrap:anywhere] [&_li]:max-w-measure [&_p]:max-w-measure [&_p]:mt-4 [&_ul]:mt-4 [&_ol]:mt-4 [&_a]:link', className)}>{children}</div>;
}
