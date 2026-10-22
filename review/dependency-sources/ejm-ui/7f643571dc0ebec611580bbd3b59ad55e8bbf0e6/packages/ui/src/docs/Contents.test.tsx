import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderToString } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { expectAccessible } from '../components/test-setup';
import { Contents, type ContentsItem } from './Contents';
import { Heading } from './Heading';

const items: ContentsItem[] = [
  { id: 'install', label: 'Install', level: 2 },
  { id: 'theme', label: 'Theme', level: 2 },
  { id: 'cookie', label: 'The cookie', level: 3 },
];
// Absolute tops of the three headings in a page 4000px tall.
const tops: Record<string, number> = { install: 100, theme: 1200, cookie: 3000 };
let scrollY = 0;

function page() {
  const view = render(
    <>
      <Contents items={items} />
      <main>
        <Heading id="install">Install</Heading>
        <Heading id="theme">Theme</Heading>
        <Heading level={3} id="cookie">The cookie</Heading>
      </main>
    </>,
  );
  for (const id of Object.keys(tops)) {
    const el = document.getElementById(id)!;
    el.getBoundingClientRect = () => ({ top: tops[id]! - scrollY }) as DOMRect;
    el.scrollIntoView = vi.fn();
  }
  return view;
}

const scrollTo = async (y: number) => {
  scrollY = y;
  Object.defineProperty(window, 'scrollY', { value: y, configurable: true });
  await act(async () => void window.dispatchEvent(new Event('scroll')));
};

describe('Contents', () => {
  afterEach(() => {
    scrollY = 0;
    Object.defineProperty(window, 'scrollY', { value: 0, configurable: true });
    Object.defineProperty(document.documentElement, 'scrollHeight', { value: 0, configurable: true });
  });

  it('is plain links to heading ids, so it jumps with no script', () => {
    const html = renderToString(<Contents items={items} />);
    for (const i of items) expect(html).toContain(`href="#${i.id}"`);
    expect(html).toContain('aria-label="On this page"');
    expect(html).not.toContain('aria-current');
  });

  it('marks the heading being read as the page scrolls, and the last one at the very end', async () => {
    Object.defineProperty(document.documentElement, 'scrollHeight', { value: 4000, configurable: true });
    const { container } = page();
    await expectAccessible(container);
    await scrollTo(0);
    await waitFor(() => expect(screen.getByRole('link', { name: 'Install' })).toHaveAttribute('aria-current', 'true'));
    expect(screen.getByRole('link', { name: 'Install' })).toHaveAttribute('aria-current', 'true');
    await scrollTo(1100);
    await waitFor(() => expect(screen.getByRole('link', { name: 'Theme' })).toHaveAttribute('aria-current', 'true'));
    expect(screen.getByRole('link', { name: 'Install' })).not.toHaveAttribute('aria-current');
    await scrollTo(4000 - window.innerHeight);
    await waitFor(() => expect(screen.getByRole('link', { name: 'The cookie' })).toHaveAttribute('aria-current', 'true'));
  });

  it('moves focus to the heading and scrolls to it when an entry is activated', async () => {
    const user = userEvent.setup();
    page();
    await user.click(screen.getByRole('link', { name: 'Theme' }));
    const heading = document.getElementById('theme')!;
    expect(heading).toHaveFocus();
    expect(heading.scrollIntoView).toHaveBeenCalled();
    expect(window.location.hash).toBe('#theme');
    expect(screen.getByRole('link', { name: 'Theme' })).toHaveAttribute('aria-current', 'true');
  });

  it('collapses an in-flow phone contents list before scrolling to its heading', async () => {
    const user = userEvent.setup();
    const { container } = render(<>
      <details open><summary>On this page</summary><Contents items={items} /></details>
      <main><Heading id="theme">Theme</Heading></main>
    </>);
    const details = container.querySelector('details')!;
    const heading = document.getElementById('theme')!;
    heading.scrollIntoView = vi.fn(() => { expect(details.open).toBe(false); });
    await user.click(screen.getByRole('link', { name: 'Theme' }));
    expect(heading.scrollIntoView).toHaveBeenCalledOnce();
    expect(heading).toHaveFocus();
    expect(window.location.hash).toBe('#theme');
  });

  it('works from the keyboard with Enter', async () => {
    const user = userEvent.setup();
    page();
    await user.tab();
    await user.keyboard('{Enter}');
    expect(document.getElementById('install')).toHaveFocus();
  });

  it('lets a modified click open the link normally', async () => {
    const user = userEvent.setup();
    page();
    await user.keyboard('{Control>}');
    await user.click(screen.getByRole('link', { name: 'Theme' }));
    expect(document.getElementById('theme')).not.toHaveFocus();
  });
});

describe('Heading', () => {
  it('links to itself with a plain anchor named for assistive technology', () => {
    render(<Heading id="x">Title</Heading>);
    expect(screen.getByRole('heading', { level: 2, name: /Title/ })).toHaveAttribute('id', 'x');
    expect(screen.getByRole('link', { name: 'Link to this section' })).toHaveAttribute('href', '#x');
  });
});
