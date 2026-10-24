import type { ReactNode } from 'react';
import { cn } from '../lib/utils';
import { ExternalLink } from './ExternalLink';
import { Brand, type SiteIdentity } from './Brand';
import { useLinkComponent, type LinkComponent, type NavItem } from './link';

export interface FooterColumn {
  heading: string;
  links: readonly NavItem[];
}

export interface FooterProps {
  /** The project identity uses the shared footer branding recipe. */
  identity?: SiteIdentity;
  /** Legacy non-site composition. Public sites supply identity data. */
  brand?: ReactNode;
  className?: string;
  columns?: readonly FooterColumn[];
  /** The privacy note, for example what analytics runs and how Do Not Track is honored. Shown above the legal line. */
  privacy?: ReactNode;
  /** The last line: copyright, a version. */
  legal?: ReactNode;
  linkComponent?: LinkComponent;
}

const linkClass = 'touch-target inline-flex items-center py-1.5 text-muted hover:text-fg';

/**
 * The site footer: a full-bleed Ink band (dark in both themes) with link columns, a slot for the privacy note and a legal
 * line. The columns are one `nav` named "Footer" (headings name the groups). Columns reflow from one to three. It keeps clear of the home bar with a safe-area inset.
 */
export function Footer({ identity, brand, className, columns = [], privacy, legal, linkComponent }: FooterProps) {
  const ambient = useLinkComponent();
  const Link = linkComponent ?? ambient;
  return (
    <footer data-band="ink" className={cn('band pt-rhythm-sm pb-[max(2.5rem,env(safe-area-inset-bottom))]', className)}>
      <div className="page">
        <div className="grid gap-x-8 gap-y-10 lg:grid-cols-[minmax(0,1.6fr)_minmax(0,3fr)]">
          {identity && <div className="max-w-measure">
            {identity.artwork ? <Brand {...identity.artwork} name={identity.name} placement="footer" /> : <p className="text-heading tracking-tight">{identity.name}</p>}
            {identity.description && <p className="mt-3 text-muted">{identity.description}</p>}
          </div>}
          {!identity && brand && <div className="max-w-measure">{brand}</div>}
          {/* One landmark for all the columns: the headings name the groups, and the name "Footer" cannot clash with a page's own navs. */}
          {columns.length > 0 && (
          <nav aria-label="Footer" className={cn('grid gap-x-8 gap-y-10 sm:grid-cols-2 lg:grid-cols-3', !identity && !brand && 'lg:col-span-2')}>
            {columns.map(col => (
              <div key={col.heading}>
                <h2 className="text-meta text-muted">{col.heading}</h2>
                <ul className="mt-3">
                  {col.links.map(l => (
                    <li key={l.href}>
                      {l.external ? (
                        <ExternalLink href={l.href} className={linkClass}>
                          {l.label}
                        </ExternalLink>
                      ) : (
                        <Link href={l.href} className={linkClass} aria-current={l.current ? 'page' : undefined}>
                          {l.label}
                        </Link>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </nav>
          )}
        </div>
        {(privacy || legal) && (
          <div className="hairline-t mt-12 grid gap-3 pt-6 text-meta text-muted">
            {privacy && <div className="max-w-measure">{privacy}</div>}
            {legal && <div>{legal}</div>}
          </div>
        )}
      </div>
    </footer>
  );
}
