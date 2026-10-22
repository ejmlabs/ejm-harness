import { defineSite } from '@ejm/ssg';
import { siteDefaults } from '@ejm/site/head';
import { App } from './frame';
import { docRoutes, pages } from './pages';
import NotFound from './routes/not-found';
import baseStyles from './styles.css?url';
import docsStyles from './styles-docs.css?url';
import appStyles from './styles-app.css?url';
import demoStyles from './styles-demos.css?url';
import productStyles from './styles-product.css?url';

const styleLinks = (path: string) => [{ rel: 'stylesheet', href: path.startsWith('/docs/') || path === '/shell/' ? docsStyles : path === '/app/' || path === '/app-shell/' ? appStyles : path === '/demos/' ? demoStyles : path === '/product/' ? productStyles : baseStyles }];

const description = 'ejm-ui is the ejmlabs design system: tokens, a Tailwind theme, components, hooks, layouts and views for every ejmlabs site and app.';
const defaults = siteDefaults('ejmlabs', { titleTemplate: '%s | ejm-ui', description });
const shotLinks = import.meta.env.SSR
  ? (await import('@ejm/ui/layout/ProductShot')).shotPreloadLinks((await import('@ejm/shots')).getShot('cli', 'tui-read-refused'))
  : [];

export default defineSite({
  ...defaults,
  // The gallery is not in the project registry yet, so it names its own head; the origin is where it is planned to be served.
  origin: 'https://ui.ejmlabs.com',
  head: { ...defaults.head, title: 'ejm-ui, the ejmlabs design system', og: { ...defaults.head.og, siteName: 'ejm-ui' } },
  App,
  routes: [
    { path: '/', head: { links: styleLinks('/') }, load: () => import('./island').then(m => m.island(() => import('./routes/home'), 'Build with ejm-ui.')) },
    // Large pages load after the small site entry. Server examples stay readable, with controls disabled until handlers are ready.
    ...[...pages, ...docRoutes].map(({ path, title, description, deferred, load }) => ({
      path,
      load: deferred ? () => import('./island').then(m => m.island(load, title)) : load,
      head: { title, description, links: [...styleLinks(path), ...(path === '/product/' ? shotLinks : [])] },
    })),
  ],
  notFound: { component: NotFound, head: { title: 'Page not found', links: styleLinks('/404') } },
});
