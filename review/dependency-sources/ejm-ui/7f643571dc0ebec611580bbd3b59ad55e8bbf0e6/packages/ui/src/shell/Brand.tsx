export interface BrandArtwork {
  src: string;
  darkSrc?: string;
  /** Intrinsic artwork dimensions preserve the supplied proportions. */
  width: number;
  height: number;
}

export interface SiteIdentity {
  name: string;
  artwork?: BrandArtwork;
  description?: string;
}

export interface BrandProps extends BrandArtwork {
  /** Omit for a wordmark. The surrounding link supplies its name. */
  name?: string;
  placement?: 'navigation' | 'project' | 'footer';
}

/** Placement owns mark size. Project names inherit the surrounding heading type. */
export function Brand({ src, darkSrc, width, height, name, placement = 'navigation' }: BrandProps) {
  const image = placement === 'project' ? 'h-10 w-auto shrink-0' : 'h-9 w-auto shrink-0';
  const paired = placement !== 'footer' && darkSrc && darkSrc !== src;
  const source = placement === 'footer' ? darkSrc ?? src : src;
  return <span className={placement === 'project' ? 'inline-flex max-w-full items-center gap-2.5 tracking-tight' : 'inline-flex max-w-full items-center gap-2.5 tracking-tight text-heading'}>
    <img src={source} width={width} height={height} alt="" aria-hidden="true" loading="lazy" decoding="async" className={paired ? `${image} dark:hidden` : image} />
    {paired && <img src={darkSrc} width={width} height={height} alt="" aria-hidden="true" loading="lazy" decoding="async" className={`${image} hidden dark:block`} />}
    {name && <span className="min-w-0">{name}</span>}
  </span>;
}
