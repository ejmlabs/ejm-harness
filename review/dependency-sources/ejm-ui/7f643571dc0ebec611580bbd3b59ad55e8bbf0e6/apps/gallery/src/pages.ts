import type { ComponentType } from 'react';
import { docMeta, docSlugs } from './content/meta';

// Every page of the gallery, in one list. site.tsx makes the routes from it, the header and footer read the labels and groups, and
// the home page makes its rows from it. To add a page: write src/routes/<name>.tsx with a default export and add one line here.
// This file imports no page code (each `load` is a dynamic import), so it adds nothing to the entry bundle.

type Load = () => Promise<{ default: ComponentType }>;

export interface GalleryPage {
  /** The path the host serves, with a trailing slash. */
  path: string;
  /** Nav, footer and row title. */
  label: string;
  /** Page title before the site template (`%s | ejm-ui`). */
  title: string;
  description: string;
  /** The mono line in a home row. */
  meta: string;
  /** The footer column. */
  group: 'Foundations' | 'Kits';
  /** Also in the header nav. */
  nav?: boolean;
  /** The page is a whole frame with its own header, skip link and main (an app shell), so the gallery adds none. */
  bare?: boolean;
  /** The page body hydrates after the small site entry. Interactive controls wait until it is ready (see island.tsx). */
  deferred?: boolean;
  load: Load;
}

export const pages: GalleryPage[] = [
  { path: '/components/', label: 'Components', title: 'Components', description: 'Every component and every state, drawn in light and dark.', meta: 'Buttons, forms, menus, tables', group: 'Kits', nav: true, deferred: true, load: () => import('./routes/components') },
  { path: '/layout/', label: 'Layout', title: 'Layout', description: 'The containerless language end to end: hero, bands, rows, facts, stats and product windows.', meta: 'Bands and rows', group: 'Kits', nav: true, deferred: true, load: () => import('./routes/layout') },
  { path: '/docs/', label: 'Docs', title: 'Docs', description: 'Getting started, the design language, tokens and bands, theming, components, and the checks.', meta: `${docSlugs.length} pages`, group: 'Foundations', nav: true, deferred: true, load: () => import('./routes/docs') },
  { path: '/app/', label: 'App', title: 'App kit', description: 'A dashboard, three chart types and every empty, loading and error state, with sample data.', meta: 'Dashboard and charts', group: 'Kits', nav: true, deferred: true, load: () => import('./routes/app') },
  { path: '/application-layout/', label: 'Application layout', title: 'Application layout', description: 'Compact page spacing, grouped form fields and every table state in both densities.', meta: 'Page and field spacing', group: 'Kits', load: () => import('./routes/application-layout') },
  { path: '/product/', label: 'Product', title: 'Product views', description: 'Real captures of ejm-cli, ejm-ide and ejm-harness, with source versions and capture dates.', meta: 'Real product captures', group: 'Kits', nav: true, deferred: true, load: () => import('./routes/product') },
  { path: '/demos/', label: 'Demos', title: 'Demos', description: 'Three read-only local playbacks of recorded product workflows, with pause and restart controls.', meta: 'Recorded workflows', group: 'Kits', deferred: true, load: () => import('./routes/demos') },
  { path: '/tokens/', label: 'Tokens', title: 'Tokens', description: 'Every band, the type scale, elevation and motion that the theme defines.', meta: 'Colors, type, depth', group: 'Foundations', load: () => import('./routes/tokens') },
  { path: '/hooks/', label: 'Hooks', title: 'Hooks and motion', description: 'Live demos of the hooks and motion primitives, each safe to render on the server.', meta: 'Theme, media, motion', group: 'Foundations', load: () => import('./routes/hooks') },
  { path: '/shell/', label: 'Shell', title: 'Shell', description: 'The header, footer and layouts that frame a site, in both themes.', meta: 'Header, footer, docs', group: 'Foundations', load: () => import('./routes/shell') },
  { path: '/app-shell/', label: 'App shell', title: 'App shell', description: 'The application frame: a top bar, a side nav and content.', meta: 'Top bar and side nav', group: 'Kits', bare: true, load: () => import('./routes/app-shell') },
];

/** The docs pages after the index, which is `/docs/` above. They share one chunk with it. */
export const docRoutes = docSlugs.slice(1).map(slug => ({
  path: `/docs/${slug}/`,
  title: `Docs: ${docMeta[slug].title}`,
  description: docMeta[slug].lead,
  deferred: true,
  load: (): ReturnType<Load> => import('./routes/docs').then(m => ({ default: m.docPages[slug]! })),
}));
