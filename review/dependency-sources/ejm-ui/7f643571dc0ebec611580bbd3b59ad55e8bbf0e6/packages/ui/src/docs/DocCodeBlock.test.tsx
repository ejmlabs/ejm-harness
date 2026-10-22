import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Suspense } from 'react';
import { hydrateRoot, type Root } from 'react-dom/client';
import { renderToString } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import { CodeBlockFrame } from './CodeBlockFrame';
import { CodeTabsFrame } from './CodeTabsFrame';
import type { DocCodeTabs } from './data';

const block: DocCodeTabs = {
  type: 'tabs',
  label: 'Install',
  tabs: [
    { label: 'macOS', lang: 'sh', code: 'brew install tool', codeLabel: 'macOS setup' },
    { label: 'Linux', lang: 'sh', code: 'apt install tool', codeLabel: 'Linux setup' },
  ],
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  vi.doUnmock('./CodeTabsFrame');
  vi.resetModules();
});

it('renders a plain code block without loading the tab module', async () => {
  vi.stubEnv('SSR', false);
  const loadTabs = vi.fn(() => vi.importActual<typeof import('./CodeTabsFrame')>('./CodeTabsFrame'));
  vi.doMock('./CodeTabsFrame', loadTabs);
  const { DocCodeBlock } = await import('./DocCodeBlock');
  render(<DocCodeBlock block={{ type: 'code', lang: 'sh', code: 'npm test', label: 'Run tests' }} />);
  expect(screen.getByRole('region', { name: 'Run tests code' })).toHaveTextContent('npm test');
  expect(loadTabs).not.toHaveBeenCalled();
});

it('retains server panels during delayed loading, then restores keyboard, copy and shared selection', async () => {
  // Use the real frames for complete server HTML. The node fixture checks the cold server entry.
  const ServerPanels = () => <Suspense fallback={null}>
    <CodeTabsFrame label={block.label} tabs={block.tabs.map((tab) => ({
      label: tab.label, children: <CodeBlockFrame code={tab.code} lang={tab.lang} label={tab.codeLabel} />,
    }))} />
  </Suspense>;
  const html = renderToString(<><ServerPanels /><ServerPanels /></>);
  expect(html).not.toContain('<template');
  expect(html).toContain('brew install tool');
  expect(html).toContain('apt install tool');
  vi.stubEnv('SSR', false);
  let release!: () => void;
  const pending = new Promise<void>((resolve) => { release = resolve; });
  const loadTabs = vi.fn(async () => {
    await pending;
    return vi.importActual<typeof import('./CodeTabsFrame')>('./CodeTabsFrame');
  });
  vi.doMock('./CodeTabsFrame', loadTabs);
  const { DocCodeBlock: BrowserBlock } = await import('./DocCodeBlock');
  const container = document.createElement('div');
  container.innerHTML = html;
  document.body.append(container);
  const before = container.textContent;
  const recoverable = vi.fn();
  const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
  const warnings = vi.spyOn(console, 'warn').mockImplementation(() => {});
  let root: Root | undefined;
  try {
    await act(async () => {
      root = hydrateRoot(container, <><BrowserBlock block={block} /><BrowserBlock block={block} /></>, { onRecoverableError: recoverable });
    });
    await waitFor(() => expect(loadTabs).toHaveBeenCalledTimes(1));
    expect(container.textContent).toBe(before);
    expect(within(container).getAllByRole('region')).toHaveLength(4);
    expect(within(container).queryByRole('tablist')).not.toBeInTheDocument();

    await act(async () => { release(); });
    const lists = await within(container).findAllByRole('tablist', { name: 'Install' });
    expect(lists).toHaveLength(2);
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    within(lists[0]!).getByRole('tab', { name: 'macOS' }).focus();
    await user.keyboard('{ArrowRight}');
    expect(within(lists[0]!).getByRole('tab', { name: 'Linux' })).toHaveFocus();
    for (const list of lists) expect(within(list).getByRole('tab', { name: 'Linux' })).toHaveAttribute('aria-selected', 'true');
    await user.click(within(container).getAllByRole('button', { name: 'Copy Linux setup code' })[0]!);
    expect(writeText).toHaveBeenCalledWith('apt install tool');
    expect(recoverable).not.toHaveBeenCalled();
    expect(errors).not.toHaveBeenCalled();
    expect(warnings).not.toHaveBeenCalled();
  } finally {
    await act(async () => { root?.unmount(); });
    container.remove();
  }
});
