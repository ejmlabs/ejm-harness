import { useId, type HTMLAttributes, type ReactNode } from 'react';

export type ShowcaseFrameProps = Omit<HTMLAttributes<HTMLDivElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'> & {
  label: string;
  note?: ReactNode;
};

/** One nearby disclosure for a passive example. Controls sit outside the product surface. */
export function ShowcaseFrame({ label, note = 'Example session. Runs in your browser without a model connection.', children, ...props }: ShowcaseFrameProps) {
  const noteId = useId();
  return <div {...props} role="group" aria-label={label} aria-describedby={note ? noteId : undefined} className="mx-auto min-w-0 max-w-6xl">
    {children}
    {note && <p id={noteId} className="mt-4 text-meta text-muted">{note}</p>}
  </div>;
}
