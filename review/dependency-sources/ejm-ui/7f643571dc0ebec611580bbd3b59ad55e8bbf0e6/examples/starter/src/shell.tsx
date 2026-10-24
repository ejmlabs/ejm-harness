import { Link, usePath } from '@ejm/ssg';
import { Analytics } from '@ejm/site/analytics';
import { otherSites, PROJECTS, sourceLinks } from '@ejm/site/projects';
import { SiteLayout, type FooterProps, type NavItem } from '@ejm/ui';
import type { ReactNode } from 'react';
import markInk from '@ejm/tokens/assets/logos/symbol-ink.svg?url';
import markPaper from '@ejm/tokens/assets/logos/symbol-paper.svg?url';

const here = PROJECTS.ejmlabs;
const identity = { name: here.name, artwork: { src: markInk, darkSrc: markPaper, width: 31, height: 28 } };

const pages = [
  { label: 'Home', href: '/' },
  { label: 'About', href: '/about/' },
];

const footer: FooterProps = {
  identity: { ...identity, description: here.tagline },
  columns: [
    { heading: 'Pages', links: pages },
    { heading: 'Sites', links: otherSites('ejmlabs') },
    { heading: 'Source', links: sourceLinks('ejmlabs') },
  ],
  privacy: 'Visits are counted with Cloudflare Web Analytics, which sets no cookies and is off when your browser sends Do Not Track or Global Privacy Control. The only cookie is your theme choice.',
  legal: `${here.name}`,
};

/** The page frame for every route: skip link, header with the theme toggle last, the page, the footer, and analytics. */
export function App({ children }: { children: ReactNode }) {
  const path = usePath();
  const nav: NavItem[] = pages.map(p => ({ ...p, current: p.href === '/' ? path === '/' : path.startsWith(p.href.replace(/\/$/, '')) }));
  return (
    <>
      <SiteLayout
        header={{ identity, homeLabel: `${here.name} home`, nav }}
        footer={footer}
        linkComponent={Link}
      >
        {children}
      </SiteLayout>
      <Analytics />
    </>
  );
}
