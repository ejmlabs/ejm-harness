import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { renderToString } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import { parseDoc, splitDoc } from '../../../site/src/docs.mjs';
import { expectAccessible } from '../components/test-setup';
import type { DocProseRun } from './data';
import { DocProse } from './DocProse';
import { DocDataBlock } from './DocDataBlock';
import { CodeHighlight } from './CodeHighlight';
import { LinkProvider, type LinkProps } from '../shell/link';
import { DocsPage, docNavigation } from './DocsPage';

const { full, client } = splitDoc(parseDoc('## Build {#build}\n\nRead the [source](/source/).\n\n```sh Run tests\nnpm test\n```\n\n| Name | CPU | OS | State |\n| --- | --- | --- | --- |\n| CLI | x64 | Linux | Ready |\n', { metadata: { title: 'Build guide', lead: 'Test the source.', group: 'Develop' } }));
const nav = [
  { href: '/docs/', title: 'Overview', group: 'Start' },
  { href: '/docs/build/', title: 'Build', group: 'Develop' },
  { href: '/docs/test/', title: 'Test', group: 'Develop' },
];
const prose = (si: number, ii: number) => <DocProse section={full.sections[si]!} blocks={(full.sections[si]!.items[ii] as DocProseRun).blocks} first={ii === 0} />;
beforeEach(() => {
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: true, addEventListener() {}, removeEventListener() {} })));
  Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockResolvedValue(undefined) }, configurable: true });
});
afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs(); });

it('keeps filtering, contents and copy live outside a prose boundary', async () => {
  const Boundary = ({ render: draw }: { render?: () => ReactNode }) => <div data-testid="prose">{draw?.()}</div>;
  const { container } = render(<DocsPage page={client} {...docNavigation(nav, '/docs/build/')} filter staticBoundary={Boundary} renderProse={prose} renderCode={(block) => <CodeHighlight code={block.code} lang={block.lang} label={block.label} />} />);
  expect(screen.getAllByTestId('prose').every((part) => !part.querySelector('nav,button,input'))).toBe(true);
  const docs = screen.getByRole('navigation', { name: 'Documentation' });
  fireEvent.change(within(docs).getByRole('searchbox'), { target: { value: 'Test' } });
  expect(within(docs).queryByRole('link', { name: 'Overview' })).not.toBeInTheDocument();
  expect(within(docs).getByRole('link', { name: 'Test' })).toHaveAttribute('href', '/docs/test/');
  fireEvent.click(screen.getByRole('button', { name: 'Copy Run tests code' }));
  expect(navigator.clipboard.writeText).toHaveBeenCalledWith('npm test');
  expect(await screen.findByRole('button', { name: 'Copied' })).toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Build' })).toHaveAttribute('href', '#build');
  const table = screen.getByRole('table');
  expect(within(table).getAllByRole('columnheader')).toHaveLength(4);
  expect(within(table).getByRole('cell', { name: 'Ready' })).toBeInTheDocument();
  await expectAccessible(container);
});

it('renders readable server HTML and keeps route paths and pager order intact', () => {
  const navigation = docNavigation(nav, '/docs/build/');
  expect(navigation.prev).toEqual({ href: '/docs/', title: 'Overview' });
  expect(navigation.next).toEqual({ href: '/docs/test/', title: 'Test' });
  const html = renderToString(<DocsPage page={client} {...navigation} renderProse={prose} />);
  expect(html).toContain('Read the');
  expect(html).toContain('/source/');
  expect(html).toContain('npm');
  expect(html).not.toContain('Copy Run tests code');
  expect(() => docNavigation(nav, '/missing/')).toThrow(/Unknown docs page/);
});

it('renders plain code inside a static boundary without a highlighting callback', () => {
  vi.stubEnv('SSR', true);
  vi.stubEnv('DEV', false);
  const Boundary = ({ render: draw }: { render?: () => ReactNode }) => <div data-ejm-static="">{draw?.()}</div>;
  const html = renderToString(<DocsPage page={client} {...docNavigation(nav, '/docs/build/')} staticBoundary={Boundary} renderProse={prose} />);
  const output = document.createElement('div');
  output.innerHTML = html;
  const code = output.querySelector('[data-ejm-static] pre code');
  expect(code?.textContent).toBe('npm test');
});

it('keeps the surrounding router adapter for sidebar, pager, and prose links', () => {
  const navigate = vi.fn();
  const Link = ({ href, ...props }: LinkProps) => <a {...props} href={href} onClick={(event) => { event.preventDefault(); navigate(href); }} />;
  render(<LinkProvider value={Link}><DocsPage page={client} {...docNavigation(nav, '/docs/build/')} renderProse={prose} /></LinkProvider>);
  fireEvent.click(screen.getByRole('link', { name: 'Overview' }));
  expect(navigate).toHaveBeenLastCalledWith('/docs/');
  fireEvent.click(screen.getByRole('link', { name: 'source' }));
  expect(navigate).toHaveBeenLastCalledWith('/source/');
  fireEvent.click(screen.getByRole('link', { name: /Next.*Test/ }));
  expect(navigate).toHaveBeenLastCalledWith('/docs/test/');
});

it('keeps nested code and compiled code tabs live through the optional rich renderer', async () => {
  const nested = splitDoc(parseDoc([
    '## Build {#build}', '', '1. Build the package:', '',
    '   ```sh title="Build package"', '   npm run build', '   ```', '',
    '> [!TIP]', '> Then check it:', '>', '> ```sh title="Check package"', '> npm test', '> ```', '',
    '<!-- tabs: Platform -->', '', '```sh tab="macOS" label="macOS setup"', 'brew install tool', '```', '',
    '```sh tab="Linux" label="Linux setup"', 'apt install tool', '```', '', '<!-- /tabs -->',
  ].join('\n'), { metadata: { title: 'Build guide' } }));
  expect(nested.requiresRich).toBe(true);
  const props = { page: nested.client, ...docNavigation(nav, '/docs/build/') };
  expect(() => renderToString(<DocsPage {...props} />)).toThrow(/needs renderBlock/);
  render(<DocsPage {...props}
    renderBlock={(block, renderCode) => <DocDataBlock block={block} renderCode={renderCode} />}
    renderProse={(si, ii) => <DocProse section={nested.full.sections[si]!} blocks={(nested.full.sections[si]!.items[ii] as DocProseRun).blocks} first={ii === 0} />} />);
  expect(screen.getByText('Build the package:')).toBeInTheDocument();
  expect(screen.getByText('Then check it:')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Copy Build package code' }));
  expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('npm run build');
  fireEvent.click(screen.getByRole('button', { name: 'Copy Check package code' }));
  expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('npm test');
  fireEvent.click(await screen.findByRole('tab', { name: 'Linux' }));
  fireEvent.click(screen.getByRole('button', { name: 'Copy Linux setup code' }));
  expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('apt install tool');
  await waitFor(() => expect(screen.getAllByRole('button', { name: 'Copied' })).toHaveLength(3));
});
