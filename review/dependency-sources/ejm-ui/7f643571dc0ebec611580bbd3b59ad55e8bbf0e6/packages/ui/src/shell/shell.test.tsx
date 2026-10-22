import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderToString } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { axe } from 'vitest-axe';
import * as axeMatchers from 'vitest-axe/matchers';
import type { AxeMatchers } from 'vitest-axe/matchers';
import { AppShell } from './AppShell';
import { Contents } from '../docs/Contents';
import { DocsSidebar } from '../docs/DocsSidebar';
import { Pager } from '../docs/Pager';
import { DocsLayout } from './DocsLayout';
import { ExternalLink } from './ExternalLink';
import { Footer } from './Footer';
import { Header } from './Header';
import type { LinkComponent } from './link';
import { SiteLayout } from './SiteLayout';
import { ThemeToggle } from './ThemeToggle';

expect.extend(axeMatchers);
const originalViewTransition = Object.getOwnPropertyDescriptor(document, 'startViewTransition');
const noViolations = async (el: HTMLElement) => (expect(await axe(el)) as unknown as AxeMatchers).toHaveNoViolations();

beforeEach(() => {
  vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.documentElement.removeAttribute('data-theme');
  localStorage.clear();
  if (originalViewTransition) Object.defineProperty(document, 'startViewTransition', originalViewTransition);
  else Reflect.deleteProperty(document, 'startViewTransition');
});

const nav = [
  { label: 'Docs', href: '/docs', current: true },
  { label: 'Gallery', href: '/gallery' },
  { label: 'GitHub', href: 'https://github.com/ejmlabs', external: true },
];
const header = { identity: { name: 'ejmlabs' }, homeLabel: 'ejmlabs home', nav };
const footer = {
  identity: { name: 'ejmlabs', description: 'Shared open source tools.' },
  columns: [{ heading: 'Product', links: nav }, { heading: 'Company', links: [{ label: 'About', href: '/about' }] }],
  privacy: <p>No cookies except your theme.</p>,
  legal: <p>2026 ejmlabs</p>,
};

describe('ExternalLink', () => {
  it('marks the link external and says so to screen readers', () => {
    render(<ExternalLink href="https://example.com">Example</ExternalLink>);
    const a = screen.getByRole('link', { name: 'Example, opens another site' });
    expect(a).toHaveAttribute('rel', 'external noopener');
    expect(a).not.toHaveAttribute('target');
    expect(a.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
  });
});

describe('ThemeToggle', () => {
  it('is named for its action, switches the theme on the root, saves it, and updates its name', async () => {
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole('button', { name: 'Switch to dark theme' });
    await user.click(button);
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(localStorage.getItem('ejm-theme')).toBe('dark');
    expect(screen.getByRole('button', { name: 'Switch to light theme' })).toBe(button);
    expect(screen.getByRole('status')).toHaveTextContent('Dark theme on');
    await user.keyboard('{Enter}');
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('saves the chosen theme when a native transition is skipped, and applies it only once', async () => {
    const user = userEvent.setup();
    let deferredUpdate: (() => void) | undefined;
    const start = vi.fn((update: () => void) => {
      deferredUpdate = update;
      return {
        ready: Promise.reject(new DOMException('Skipped', 'AbortError')),
        updateCallbackDone: Promise.resolve(),
        finished: Promise.resolve(),
        skipTransition: vi.fn(),
      };
    });
    Object.defineProperty(document, 'startViewTransition', { configurable: true, value: start });
    render(<ThemeToggle />);
    const button = screen.getByRole('button', { name: /Switch to .* theme/ });
    const target = button.getAttribute('aria-label')!.includes('dark') ? 'dark' : 'light';
    await user.click(button);
    expect(start).toHaveBeenCalledOnce();
    expect(document.documentElement.dataset.theme).toBe(target);
    expect(localStorage.getItem('ejm-theme')).toBe(target);
    deferredUpdate?.();
    expect(document.documentElement.dataset.theme).toBe(target);
    await user.click(button);
  });

  it('bypasses native transitions when reduced motion is requested', async () => {
    vi.stubGlobal('matchMedia', (query: string) => ({ matches: query.includes('prefers-reduced-motion'), addEventListener() {}, removeEventListener() {} }));
    const start = vi.fn();
    Object.defineProperty(document, 'startViewTransition', { configurable: true, value: start });
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole('button', { name: /Switch to .* theme/ });
    const target = button.getAttribute('aria-label')!.includes('dark') ? 'dark' : 'light';
    await user.click(button);
    expect(start).not.toHaveBeenCalled();
    expect(document.documentElement.dataset.theme).toBe(target);
    await user.click(button);
  });

  it('switches immediately if the native transition cannot start', async () => {
    Object.defineProperty(document, 'startViewTransition', { configurable: true, value: () => { throw new Error('Unavailable'); } });
    const user = userEvent.setup();
    render(<ThemeToggle />);
    const button = screen.getByRole('button', { name: /Switch to .* theme/ });
    const target = button.getAttribute('aria-label')!.includes('dark') ? 'dark' : 'light';
    await user.click(button);
    expect(document.documentElement.dataset.theme).toBe(target);
    expect(localStorage.getItem('ejm-theme')).toBe(target);
    await user.click(button);
  });

  it('server-renders as a button with the light-theme name, before any script runs', () => {
    expect(renderToString(<ThemeToggle />)).toContain('aria-label="Switch to dark theme"');
  });
});

describe('Header', () => {
  it('puts the toggle last, marks the current page, and links external items as external', () => {
    render(<Header {...header} />);
    const banner = screen.getByRole('banner');
    const focusable = [...banner.querySelectorAll('a, button, summary')];
    expect(focusable[focusable.length - 1]).toHaveAccessibleName('Switch to dark theme');
    expect(within(banner).getAllByRole('link', { name: 'Docs' })[0]).toHaveAttribute('aria-current', 'page');
    expect(within(banner).getAllByRole('link', { name: /GitHub/ })[0]).toHaveAttribute('rel', 'external noopener');
    expect(within(banner).getByRole('link', { name: 'ejmlabs home' })).toHaveAttribute('href', '/');
  });

  it('opens the narrow menu from the keyboard, closes on Escape with focus back on the button, and closes after choosing a link', async () => {
    const user = userEvent.setup();
    const { container } = render(<Header {...header} />);
    const details = container.querySelector('details')!;
    const summary = details.querySelector('summary')!;
    await user.click(summary);
    expect(details.open).toBe(true);
    await user.keyboard('{Escape}');
    expect(details.open).toBe(false);
    expect(summary).toHaveFocus();
    await user.click(summary);
    expect(details.open).toBe(true);
    await user.click(within(details).getByRole('link', { name: 'Gallery' }));
    expect(details.open).toBe(false);
  });

  it('closes the menu on a press outside it', async () => {
    const user = userEvent.setup();
    const { container } = render(<><Header {...header} /><p>elsewhere</p></>);
    const details = container.querySelector('details')!;
    await user.click(details.querySelector('summary')!);
    expect(details.open).toBe(true);
    await user.click(screen.getByText('elsewhere'));
    expect(details.open).toBe(false);
  });

  it('uses the supplied link component for internal links only', () => {
    const Router: LinkComponent = ({ href, ...p }) => <a data-router href={href} {...p} />;
    const { container } = render(<Header {...header} linkComponent={Router} />);
    const routed = [...container.querySelectorAll('a[data-router]')].map(a => a.getAttribute('href'));
    expect(routed).toContain('/docs');
    expect(routed).not.toContain('https://github.com/ejmlabs');
  });

  it('has no axe violations', async () => {
    const { container } = render(<Header {...header} />);
    await noViolations(container);
  });
});

describe('landmarks', () => {
  // A docs page as a site composes it: header, sidebar, contents, pager and a footer whose column is also called "Documentation".
  const page = (
    <SiteLayout
      header={header}
      footer={{ ...footer, columns: [{ heading: 'Documentation', links: nav }, { heading: 'Product', links: nav }] }}
    >
      <DocsLayout
        sidebar={<DocsSidebar groups={[{ title: 'Start', items: [{ label: 'Getting started', href: '/docs' }] }]} current="/docs" />}
        contents={<Contents items={[{ id: 'a', label: 'A', level: 2 }]} />}
      >
        <h1>Docs</h1>
        <Pager next={{ href: '/docs/next', title: 'Next' }} />
      </DocsLayout>
    </SiteLayout>
  );

  it('gives every navigation landmark its own name, with one in the header and one in the footer', () => {
    render(page);
    const names = screen.getAllByRole('navigation').map(n => n.getAttribute('aria-label'));
    expect(names.sort()).toEqual(['Documentation', 'Footer', 'Main', 'On this page', 'Previous and next pages']);
    expect(new Set(names).size).toBe(names.length);
  });

  it('passes axe, including landmark-unique, as a whole page', async () => {
    const { container } = render(page);
    await noViolations(container);
  });
});

describe('Footer', () => {
  it('is an Ink band with labelled link columns and the privacy slot', () => {
    render(<Footer {...footer} />);
    const f = screen.getByRole('contentinfo');
    expect(f).toHaveAttribute('data-band', 'ink');
    const columns = within(within(f).getByRole('navigation', { name: 'Footer' }));
    expect(columns.getByRole('heading', { name: 'Product' })).toBeInTheDocument();
    expect(columns.getByRole('heading', { name: 'Company' })).toBeInTheDocument();
    expect(within(f).getByText('No cookies except your theme.')).toBeInTheDocument();
    expect(within(f).getByRole('link', { name: /GitHub/ })).toHaveAttribute('rel', 'external noopener');
  });

  it('has no axe violations', async () => {
    const { container } = render(<Footer {...footer} />);
    await noViolations(container);
  });
});

describe('SiteLayout', () => {
  it('has the skip link first, one main with id and focus target, and the landmarks in order', async () => {
    const { container } = render(<SiteLayout header={header} footer={footer}><h1>Title</h1></SiteLayout>);
    const links = screen.getAllByRole('link');
    expect(links[0]).toHaveAttribute('href', '#main');
    const main = screen.getByRole('main');
    expect(main).toHaveAttribute('id', 'main');
    expect(main).toHaveAttribute('tabindex', '-1');
    const order = [...container.querySelectorAll('header, main, footer')].map(e => e.tagName);
    expect(order).toEqual(['HEADER', 'MAIN', 'FOOTER']);
    await noViolations(container);
  });

  it('passes the link component down to the header and footer', () => {
    const Router: LinkComponent = ({ href, ...p }) => <a data-router href={href} {...p} />;
    const { container } = render(<SiteLayout header={header} footer={footer} linkComponent={Router}>x</SiteLayout>);
    expect(container.querySelectorAll('header a[data-router]').length).toBeGreaterThan(0);
    expect(container.querySelectorAll('footer a[data-router]').length).toBeGreaterThan(0);
  });

  it('renders all of its content on the server, with no scripts', () => {
    const html = renderToString(<SiteLayout header={header} footer={footer}><h1>Visible</h1></SiteLayout>);
    expect(html).toContain('Visible');
    expect(html).toContain('Skip to main content');
    expect(html).not.toContain('data-reveal');
    expect(html).not.toContain(' hidden=""');
  });
});

describe('DocsLayout', () => {
  it('collapses sidebar and contents into native labelled disclosures, once each, and keeps source order', async () => {
    const { container } = render(
      <DocsLayout sidebar={<ul><li><a href="/a">Page A</a></li></ul>} contents={<ul><li><a href="#x">Heading X</a></li></ul>}>
        <article><h1>Doc</h1></article>
      </DocsLayout>,
    );
    const summaries = [...container.querySelectorAll('details > summary')].map(s => s.textContent);
    expect(summaries).toEqual(['Browse docs', 'On this page']);
    expect(screen.getAllByText('Page A')).toHaveLength(1);
    const text = container.textContent ?? '';
    expect(text.indexOf('Page A')).toBeLessThan(text.indexOf('Heading X'));
    expect(text.indexOf('Heading X')).toBeLessThan(text.indexOf('Doc'));
    await noViolations(container);
  });

  it('omits the contents disclosure when there are none', () => {
    const { container } = render(<DocsLayout sidebar={<p>nav</p>}>x</DocsLayout>);
    expect(container.querySelectorAll('details')).toHaveLength(1);
  });

  it('where ::details-content is unsupported, opens the details on a wide screen', () => {
    vi.stubGlobal('CSS', { supports: () => false });
    vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener() {}, removeEventListener() {} }));
    const { container } = render(<DocsLayout sidebar={<p>nav</p>}>x</DocsLayout>);
    expect(container.querySelector('details')!.open).toBe(true);
  });
});

describe('AppShell', () => {
  it('has a top bar with the toggle last, a labelled side nav and main', async () => {
    const { container } = render(
      <AppShell brand={<span>ejm-ide</span>} nav={<ul><li><a href="/files">Files</a></li></ul>} actions={<button type="button">Search</button>}>
        <h1>Workspace</h1>
      </AppShell>,
    );
    const bar = screen.getByRole('banner');
    const buttons = within(bar).getAllByRole('button');
    expect(buttons.map(b => b.getAttribute('aria-label') ?? b.textContent)).toEqual(['Search', 'Switch to dark theme']);
    expect(screen.getByRole('navigation', { name: 'Navigation' })).toBeInTheDocument();
    expect(screen.getByRole('main')).toHaveAttribute('id', 'main');
    expect(screen.getAllByRole('link')[0]).toHaveAttribute('href', '#main');
    await noViolations(container);
  });
});
