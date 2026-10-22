import { Menu, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { cn } from '../lib/utils';
import type { Band } from '@ejm/tokens';
import { ExternalLink } from './ExternalLink';
import { useLinkComponent, type LinkComponent, type NavItem } from './link';
import { ThemeToggle } from './ThemeToggle';
import { Brand, type SiteIdentity } from './Brand';

export interface HeaderProps {
  /** Shared branding renders the project identity inside the home link. */
  identity?: SiteIdentity;
  /** Legacy non-site composition. Public sites supply identity data. */
  logo?: ReactNode;
  themeControl?: ReactNode;
  className?: string;
  /** The accessible name of the home link. */
  homeLabel?: string;
  homeHref?: string;
  nav?: readonly NavItem[];
  /** Matches the band the page opens with. Default: the page ground. */
  band?: Band;
  linkComponent?: LinkComponent;
}

const itemClass =
  'touch-target inline-flex items-center rounded-control px-3 py-2 text-fg transition-colors duration-base ease-standard hover:bg-tone hover:text-link aria-[current=page]:font-semibold aria-[current=page]:underline aria-[current=page]:decoration-2 aria-[current=page]:underline-offset-[0.5em]';

/**
 * The site header, sticky on the page ground with no box or rule. Logo at the left; at the right the nav, then the
 * ThemeToggle, always last. The nav is one landmark. Below 768px its list folds into a native <details> menu, so it works without scripts and
 * takes no space when closed (the open menu is an overlay, so nothing moves). It sets the sticky offset
 * through --ejm-header-h and keeps clear of the notch with safe-area insets.
 */
export function Header({ identity, logo, themeControl, className, homeLabel = 'Home', homeHref = '/', nav = [], band, linkComponent }: HeaderProps) {
  const ambient = useLinkComponent();
  const Link = linkComponent ?? ambient;
  const menu = useRef<HTMLDetailsElement>(null);
  const [open, setOpen] = useState(false);

  // While the menu is open: Escape closes it and returns focus to the button, and so does a press anywhere outside.
  useEffect(() => {
    const d = menu.current;
    if (!open || !d) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      d.open = false;
      d.querySelector('summary')?.focus();
    };
    const onPress = (e: Event) => {
      if (!d.contains(e.target as Node)) d.open = false;
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onPress);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', onPress);
    };
  }, [open]);

  const renderItem = (item: NavItem, cls: string) =>
    item.external ? (
      <ExternalLink href={item.href} className={cls} aria-current={item.current ? 'page' : undefined}>
        {item.label}
      </ExternalLink>
    ) : (
      <Link href={item.href} className={cls} aria-current={item.current ? 'page' : undefined}>
        {item.label}
      </Link>
    );

  return (
    <header
      data-band={band}
      className={cn('band sticky top-0 z-40 pt-[env(safe-area-inset-top)]', className)}
    >
      <div className="page-inset flex h-(--ejm-header-h) items-center justify-between gap-4">
        <Link href={homeHref} aria-label={homeLabel} className="touch-target -ms-2 inline-flex items-center rounded-control px-2 py-2 text-fg">
          {identity ? (identity.artwork ? <Brand {...identity.artwork} name={identity.name} /> : <span className="text-heading tracking-tight">{identity.name}</span>) : logo}
        </Link>

        <div className="flex items-center gap-1">
          {nav.length > 0 && (
            // One landmark. Above 768px the list is shown and the disclosure is display:none; below it the other way round, so only one
            // of them is ever in the accessibility tree.
            <nav aria-label="Main">
              <ul className="hidden items-center gap-1 md:flex">
                {nav.map(item => (
                  <li key={item.href}>{renderItem(item, itemClass)}</li>
                ))}
              </ul>
              <details ref={menu} className="group md:hidden" onToggle={e => setOpen(e.currentTarget.open)}>
                <summary className="touch-target inline-flex min-h-11 cursor-pointer list-none items-center gap-2 rounded-control px-3 text-fg transition-colors duration-base ease-standard hover:bg-tone hover:text-link marker:hidden [&::-webkit-details-marker]:hidden">
                  <span>Menu</span>
                  <Menu aria-hidden className="size-5 group-open:hidden" />
                  <X aria-hidden className="hidden size-5 group-open:block" />
                </summary>
                <div
                  onClick={e => {
                    if ((e.target as Element).closest('a') && menu.current) menu.current.open = false;
                  }}
                  className="page-inset absolute inset-x-0 top-full max-h-[calc(100svh-var(--ejm-header-h)-env(safe-area-inset-top))] overflow-y-auto overscroll-contain bg-ground pb-6 shadow-raised group-open:animate-fade motion-reduce:animate-none"
                >
                  <ul>
                    {nav.map(item => (
                      <li key={item.href} className="hairline-t">
                        {renderItem(item, 'touch-target flex min-h-12 items-center text-heading text-fg transition-colors duration-base ease-standard hover:bg-tone hover:text-link aria-[current=page]:underline aria-[current=page]:decoration-2 aria-[current=page]:underline-offset-[0.3em]')}
                      </li>
                    ))}
                  </ul>
                </div>
              </details>
            </nav>
          )}
          {themeControl === undefined ? <ThemeToggle /> : themeControl}
        </div>
      </div>
    </header>
  );
}
