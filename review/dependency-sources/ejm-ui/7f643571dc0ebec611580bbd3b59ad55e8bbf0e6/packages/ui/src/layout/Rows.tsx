import { contract, type RowState } from '@ejm/core';
import { ArrowRight, ArrowUpRight } from 'lucide-react';
import { createContext, useContext, type ReactNode } from 'react';
import { cn } from '../lib/utils';
import { useLinkComponent } from '../shell/link';

type RowSize = 'heading' | 'title';
const SizeContext = createContext<RowSize>('heading');

export interface RowsProps {
  /** Row title size: heading (20 to 32px) or title (28 to 56px, for a short list of projects). */
  size?: RowSize;
  'aria-label'?: string;
  className?: string;
  children: ReactNode;
}

/**
 * A hairline list that replaces cards. Rows lay out in columns from 42rem of the list's own width.
 * The list extends 1rem past its column on each side so row text stays on the column edge and the hover tone and hairlines reach the gutter.
 */
export function Rows({ size = 'heading', className, children, ...rest }: RowsProps) {
  return (
    <SizeContext.Provider value={size}>
      <ul className={cn('@container hairline-t -mx-4 m-0 list-none p-0', className)} {...rest}>{children}</ul>
    </SizeContext.Provider>
  );
}

export interface RowProps {
  title: ReactNode;
  description?: ReactNode;
  /** Mono metadata, for example "Rust / MIT". */
  meta?: ReactNode;
  /** When set the whole row is the link, through its title. */
  href?: string;
  /** Another site: rel="external", the diagonal arrow and "opens another site" for screen readers. */
  external?: boolean;
  /** The link downloads the file instead of navigating. Pass a string to suggest the file name. */
  download?: boolean | string;
  state?: RowState;
  className?: string;
}

const focusedShape = (
  <svg aria-hidden="true" viewBox="0 0 8 10" className="inline-block h-[0.7em] w-[0.55em] shrink-0 fill-current"><path d="M0 0l8 5-8 5z" /></svg>
);

/** One row of a Rows list: title, description, mono meta and an arrow. Hover shifts the tone and slides the arrow. */
export function Row({ title, description, meta, href, external, download, state = 'default', className }: RowProps) {
  const size = useContext(SizeContext);
  const Link = useLinkComponent();
  const disabled = state === 'disabled';
  const link = href && !disabled;
  const Arrow = external ? ArrowUpRight : ArrowRight;
  const marker = state === 'selected' ? <span aria-hidden="true" className="font-mono">{contract.glyphs.selected.unicode}</span> : state === 'focused' ? focusedShape : null;
  const titleClass = cn(size === 'title' ? 'text-title' : 'text-heading', (state === 'selected' || state === 'focused') && 'text-fg', disabled && 'text-muted');
  return (
    <li
      aria-current={state === 'selected' ? 'true' : undefined}
      data-state={state}
      className={cn(
        'group hairline relative isolate grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-6 gap-y-2 px-4 py-6',
        '@2xl:grid-cols-[minmax(0,4fr)_minmax(0,5fr)_minmax(0,3fr)_1.5rem] @2xl:gap-x-8 @2xl:py-8',
        // The hover tone is an opacity layer behind the content, so the shift is transform and opacity only.
        'before:absolute before:inset-0 before:-z-10 before:bg-tone before:opacity-0 before:transition-opacity before:duration-base',
        link && 'hover:before:opacity-100 has-[:focus-visible]:before:opacity-100 has-[:focus-visible]:outline-2 has-[:focus-visible]:-outline-offset-2 has-[:focus-visible]:outline-focus',
        state === 'selected' && 'before:opacity-100',
        state === 'focused' && 'before:opacity-100 outline-2 -outline-offset-2 outline-focus',
        className,
      )}
    >
      <div className={cn(titleClass, 'flex items-baseline gap-3')}>
        {marker}
        {link ? (
          <Link
            href={href}
            rel={external ? 'external' : undefined}
            download={download}
            className="touch-target min-w-0 after:absolute after:inset-0 focus-visible:outline-hidden"
          >
            {title}
            {external && <span className="sr-inline">, opens another site</span>}
          </Link>
        ) : (
          <span className="min-w-0">{title}</span>
        )}
      </div>
      {description && <p className={cn('col-span-2 m-0 max-w-measure text-muted @2xl:col-span-1')}>{description}</p>}
      {(meta || disabled) && (
        <p className="text-meta col-span-2 m-0 text-muted @2xl:col-span-1">
          {meta}
          {disabled && <span className="ms-3">{contract.text.row.disabled}</span>}
        </p>
      )}
      {link ? (
        <Arrow aria-hidden="true" strokeWidth={1.5} className="col-start-2 row-start-1 size-6 justify-self-end transition-transform duration-base ease-standard group-hover:translate-x-1 group-has-[:focus-visible]:translate-x-1 @2xl:col-start-4 @2xl:row-start-auto @2xl:self-center" />
      ) : null}
    </li>
  );
}
