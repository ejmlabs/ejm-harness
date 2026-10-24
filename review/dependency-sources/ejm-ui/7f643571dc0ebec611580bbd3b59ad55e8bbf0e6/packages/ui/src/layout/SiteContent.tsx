import type { HTMLAttributes, ReactNode, Ref } from 'react';
import { cn } from '../lib/utils';

/** Content, DOM identity and accessible state may cross the site boundary. Visual overrides may not. */
export type SiteContentProps = Omit<HTMLAttributes<HTMLDivElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'>;
type ContentProps = { children?: ReactNode };

/** The space between a section opening and its content. */
export function SectionBody(props: SiteContentProps) {
  return <div {...props} className="mt-16" />;
}

/** Explanatory content precedes supporting rows or facts, at one shared proportion. */
export function Columns({ primary, secondary }: { primary: ReactNode; secondary: ReactNode }) {
  return <div className="grid gap-x-12 gap-y-12 lg:grid-cols-12">
    <div className="min-w-0 lg:col-span-5">{primary}</div>
    <div className="min-w-0 lg:col-span-7">{secondary}</div>
  </div>;
}

/** Comparable figures share one grid, rather than consumer-defined breakpoints. */
export function MetricGrid(props: SiteContentProps) {
  return <div {...props} className="grid gap-x-12 gap-y-10 sm:grid-cols-2 lg:grid-cols-4" />;
}

/** Readable prose, status and actions follow one content rhythm. */
export function ContentFlow(props: SiteContentProps) {
  return <div {...props} className="grid min-w-0 max-w-measure grid-cols-1 gap-6" />;
}

/** A heading, text and its related action form a single group. */
export function ContentGroup(props: SiteContentProps) {
  return <div {...props} className="grid min-w-0 grid-cols-1 gap-4" />;
}

/** Source examples use the same bounded reading width on every site. */
export function ReadingColumn(props: SiteContentProps) {
  return <div {...props} className="min-w-0 max-w-3xl" />;
}

export function Actions(props: SiteContentProps) {
  return <div {...props} className="flex flex-wrap items-center gap-x-8 gap-y-4" />;
}

/** Qualifications stay near the content they describe, with the same type and spacing. */
export function Note(props: Omit<HTMLAttributes<HTMLParagraphElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'>) {
  return <p {...props} className="mt-8 max-w-measure text-meta text-muted" />;
}

export function ContentHeading({ as: H = 'h3', id, children }: ContentProps & { as?: 'h2' | 'h3' | 'h4'; id?: string }) {
  return <H id={id} className="text-heading">{children}</H>;
}

export { InlineLink } from './InlineLink';

export function InlineCode({ children }: ContentProps) {
  return <code className="font-mono text-[0.9em]">{children}</code>;
}

/** Plain source commands do not need a client highlighter. */
export function CommandLines({ commands }: { commands: readonly string[] }) {
  return <pre className="max-w-measure whitespace-pre-wrap break-words font-mono text-meta"><code>{commands.join('\n')}</code></pre>;
}

export function VisuallyHidden({ children }: ContentProps) {
  return <span className="sr-only">{children}</span>;
}

/** Release size and checksum values must wrap inside narrow fact rows. */
export function FileDetail({ children }: ContentProps) {
  return <span className="mt-1 block break-all text-meta text-muted tabular-nums">{children}</span>;
}

/** A programmatic focus target retains the global visible focus rule. */
export function FocusRegion({ ref, ...props }: SiteContentProps & { ref?: Ref<HTMLDivElement> }) {
  return <div {...props} ref={ref} />;
}

/** Artwork uses a single flexible grid, whatever number of files a page supplies. */
export function AssetGrid({ children }: ContentProps) {
  return <div className="grid gap-12 sm:grid-cols-[repeat(auto-fit,minmax(15rem,1fr))]">{children}</div>;
}

export interface BrandAssetProps {
  background: 'paper' | 'ink';
  src: string;
  width: number;
  height: number;
  alt: string;
  caption?: ReactNode;
  action?: ReactNode;
}

/** File previews retain their intrinsic proportions and a common display height. */
export function BrandAsset({ background, src, width, height, alt, caption, action }: BrandAssetProps) {
  return <figure>
    <div className={cn('p-8', background === 'ink' ? 'bg-ink' : 'bg-paper')}>
      <img src={src} width={width} height={height} alt={alt} loading="lazy" decoding="async" className="h-24 w-full object-contain" />
    </div>
    {(caption || action) && <figcaption className="mt-6 grid gap-3 text-body">{caption && <div>{caption}</div>}{action && <div>{action}</div>}</figcaption>}
  </figure>;
}

const palette = { sage: 'bg-sage', forest: 'bg-forest', ink: 'bg-ink', paper: 'bg-paper', mist: 'bg-mist' };
export function PaletteSwatch({ color, children }: ContentProps & { color: keyof typeof palette }) {
  return <span className="inline-flex items-center gap-4"><span aria-hidden="true" className={cn('inline-block h-8 w-8', palette[color])} />{children}</span>;
}
