import type { HTMLAttributes } from 'react';

export interface ProseProps extends Omit<HTMLAttributes<HTMLElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'> {
  as?: 'article' | 'div';
}

/** Plain semantic reading content uses the shared rhythm without a client highlighter. */
export function Prose({ as: Tag = 'div', ...props }: ProseProps) {
  return <Tag {...props} className="grid min-w-0 max-w-measure gap-6 text-body [overflow-wrap:anywhere] [&_a]:link [&_ul]:list-disc [&_ul]:ps-6 [&_ol]:list-decimal [&_ol]:ps-6 [&_li+li]:mt-2 [&_code]:font-mono" />;
}
