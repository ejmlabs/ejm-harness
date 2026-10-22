import { contract } from '@ejm/core';
import { render, screen, within } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { LinkProvider } from '../shell/link';
import { expectAccessible, renderIn, themes } from '../components/test-setup';
import { Band, Facts, Fact, Hero, Page, ProductStage, ProductWindow, Row, Rows, Rule, SectionHead, Stat, Status, Text } from './index';

describe('application layout', () => {
  it('keeps the default column bare and lets application callers override spacing', () => {
    const { container, rerender } = render(<Page aria-label="Games"><p>History</p></Page>);
    const page = container.firstElementChild!;
    expect(page).toHaveAttribute('aria-label', 'Games');
    expect(page).toHaveClass('page');
    for (const token of ['grid', 'gap-content', 'py-8']) expect(page).not.toHaveClass(token);

    rerender(<Page variant="application"><p>History</p></Page>);
    expect(page).toHaveClass('page', 'grid', 'gap-content', 'py-8', 'sm:py-12');
    expect(page).not.toHaveAttribute('variant');

    rerender(<Page variant="application" className="gap-4 py-4 sm:py-6"><p>History</p></Page>);
    expect(page).toHaveClass('gap-4', 'py-4', 'sm:py-6');
    for (const token of ['gap-content', 'py-8', 'sm:py-12']) expect(page).not.toHaveClass(token);
  });

  it('selects a complete recipe by purpose while keeping heading semantics and copy links', () => {
    const copy = <p>Read <a href="/help/">help</a>.</p>;
    const { rerender } = render(<SectionHead purpose="page" kicker="Your account" as="h1" id="account-title" title="Sign in">{copy}</SectionHead>);
    expect(screen.getByRole('heading', { level: 1, name: 'Sign in' })).toHaveAttribute('id', 'account-title');
    expect(screen.getByRole('heading', { level: 1 })).toHaveClass('text-display');
    expect(screen.getByRole('link', { name: 'help' }).closest('.text-lead')).not.toBeNull();

    rerender(<SectionHead purpose="application" as="h1" id="account-title" title="Sign in">{copy}</SectionHead>);
    const title = screen.getByRole('heading', { level: 1, name: 'Sign in' });
    expect(title).toHaveAttribute('id', 'account-title');
    expect(title).toHaveClass('text-title');
    expect(title.parentElement?.tagName).toBe('HEADER');
    expect(screen.getByRole('link', { name: 'help' })).toHaveAttribute('href', '/help/');
    expect(screen.getByRole('link', { name: 'help' }).closest('.text-body')).not.toBeNull();

    rerender(<SectionHead as="h2" title="Recovery" />);
    expect(screen.getByRole('heading', { level: 2, name: 'Recovery' })).toHaveClass('text-title');
  });
});

describe('Band', () => {
  it('sets its ground and wraps children in the page column unless told not to', () => {
    const { container, rerender } = render(<Band tone="sage"><p>in column</p></Band>);
    const band = container.firstElementChild!;
    expect(band).toHaveAttribute('data-band', 'sage');
    expect(band.querySelector('.page')).toContainElement(screen.getByText('in column'));
    rerender(<Band tone="ink" column={false}><p>bare</p></Band>);
    expect(container.querySelector('.page')).toBeNull();
  });
});

describe('Rows and Row', () => {
  it('uses the site link contract for route and download rows', () => {
    const Adapter = (props: import('../shell/link').LinkProps) => <a {...props} data-router="site" />;
    render(<LinkProvider value={Adapter}><Rows><Row title="Docs route" href="/docs/" /><Row title="File" href="/kit.zip" download /></Rows></LinkProvider>);
    expect(screen.getByRole('link', { name: 'Docs route' })).toHaveAttribute('data-router', 'site');
    expect(screen.getByRole('link', { name: 'File' })).toHaveAttribute('download', '');
  });

  it('makes the whole row one link, named by its title, with rel external on another site', () => {
    render(
      <Rows aria-label="Projects">
        <Row title="ejm-cli" href="https://example.test/cli" external description="A terminal agent." meta="Rust" />
        <Row title="Docs" href="/docs" />
      </Rows>,
    );
    const links = screen.getAllByRole('link');
    expect(links).toHaveLength(2);
    expect(screen.getByRole('link', { name: 'ejm-cli, opens another site' })).toHaveAttribute('rel', 'external');
    expect(screen.getByRole('link', { name: 'Docs' })).not.toHaveAttribute('rel');
  });

  it('passes download to the link, and says "opens another site" only for an external row', () => {
    render(
      <Rows aria-label="Files">
        <Row title="Brand kit" href="/brand.zip" download="ejm-brand.zip" />
        <Row title="Source" href="https://example.test/src" external download />
        <Row title="Plain" href="/plain" />
      </Rows>,
    );
    expect(screen.getByRole('link', { name: 'Brand kit' })).toHaveAttribute('download', 'ejm-brand.zip');
    expect(screen.getByRole('link', { name: 'Source, opens another site' })).toHaveAttribute('download', '');
    expect(screen.getByRole('link', { name: 'Plain' })).not.toHaveAttribute('download');
    expect(screen.getByRole('link', { name: 'Brand kit' }).textContent).not.toContain('another site');
  });

  it('keeps the screen reader text in the flow (not absolutely positioned) so it cannot widen a table or a scroll region', () => {
    const { container } = render(<Rows aria-label="x"><Row title="Out" href="https://example.test" external /></Rows>);
    const note = container.querySelector('.sr-inline')!;
    expect(note.className).not.toContain('sr-only');
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'base.css'), 'utf8');
    expect(css.match(/@utility sr-inline \{[^}]*\}/)![0]).not.toContain('position');
  });

  it('names every state in words or glyphs, never color alone', () => {
    render(
      <Rows aria-label="States">
        <Row title="chosen" state="selected" href="#a" />
        <Row title="held" state="focused" href="#b" />
        <Row title="off" state="disabled" href="#c" />
      </Rows>,
    );
    const [selected, , disabled] = screen.getAllByRole('listitem');
    expect(selected).toHaveAttribute('aria-current', 'true');
    expect(selected).toHaveTextContent(contract.glyphs.selected.unicode);
    expect(within(disabled!).queryByRole('link')).toBeNull();
    expect(disabled).toHaveTextContent(contract.text.row.disabled);
  });
});

describe('Status', () => {
  it.each(Object.keys(contract.glyphs.status) as (keyof typeof contract.glyphs.status)[])('%s shows its contract glyph and its word together', level => {
    const { container } = render(<Status status={level} />);
    const g = contract.glyphs.status[level];
    expect(container).toHaveTextContent(g.unicode + g.label);
    expect(container.querySelector('[aria-hidden="true"]')).toHaveTextContent(g.unicode);
  });
});

describe('Stat', () => {
  it('replaces the figure with the contract word for loading, error and empty', () => {
    const { rerender } = render(<Stat figure="42" label="sessions" />);
    expect(screen.getByText('42')).toBeInTheDocument();
    for (const state of ['loading', 'error', 'empty'] as const) {
      rerender(<Stat figure="42" label="sessions" state={state} />);
      expect(screen.queryByText('42')).toBeNull();
      expect(screen.getByText(contract.text.stat[state])).toBeInTheDocument();
      expect(screen.getByText('sessions')).toBeInTheDocument();
    }
  });
});

describe('ProductWindow', () => {
  it('is decoration by default and carries the Simulated screen label', () => {
    render(<ProductWindow title="ejm-cli">output</ProductWindow>);
    expect(screen.queryByRole('group')).toBeNull();
    expect(document.querySelector('[data-window]')).toHaveAttribute('aria-hidden', 'true');
    expect(screen.getAllByText('Simulated screen').length).toBeGreaterThan(0);
  });
  it('gets a name and role when interactive, and drops the label only for a live demo', () => {
    render(<ProductWindow title="ejm-ide" label="ejm-ide demo" live>live</ProductWindow>);
    expect(screen.getByRole('group', { name: 'ejm-ide demo' })).toBeInTheDocument();
    expect(screen.queryByText('Simulated screen')).toBeNull();
  });
});

describe('ProductStage', () => {
  it('layers at most three windows', () => {
    const w = (n: number) => <ProductWindow key={n} title={`w${n}`}>x</ProductWindow>;
    const { container } = render(<ProductStage>{[1, 2, 3, 4].map(w)}</ProductStage>);
    expect(container.querySelectorAll('[data-window]')).toHaveLength(3);
  });
});

describe('Hero', () => {
  it('names its region and keeps content present before its shared entrance', () => {
    const { container } = render(<Hero title="Product" titleId="product-title" footnote={<p>Availability</p>} aside={<p>Preview</p>}>Description</Hero>);
    expect(screen.getByRole('region', { name: 'Product' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1 })).toHaveAttribute('id', 'product-title');
    expect(screen.getByText('Availability')).toBeInTheDocument();
    expect(container.querySelector('.animate-slide')).not.toBeNull();
  });

  it('has one h1, puts the copy before the aside, and hides the decorative art', () => {
    const { container } = render(
      <Hero title="Small experiments" aside={<p>aside</p>} art="<svg><path d='M0 0'/></svg>" actions={<a href="#x">Go</a>}>
        <p>copy</p>
      </Hero>,
    );
    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    const copy = screen.getByText('copy');
    const aside = screen.getByText('aside');
    expect(copy.compareDocumentPosition(aside) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(container.querySelector('svg')!.closest('[aria-hidden="true"]')).not.toBeNull();
    // Nothing is hidden before scripts run: the draw-on class is only added after mount.
    expect(container.querySelector('.draw-on')).toBeNull();
  });

  it('never fades the headline or the lead (they are the LCP), and fades the actions and the aside after them', () => {
    render(
      <Hero eyebrow="lab" title="Small experiments" aside={<p>aside</p>} actions={<a href="#x">Go</a>}>
        <p>copy</p>
      </Hero>,
    );
    const entrance = (el: HTMLElement) => el.closest<HTMLElement>('[class*="animate-"]')!;
    for (const el of [screen.getByRole('heading', { level: 1 }), screen.getByText('copy'), screen.getByText('lab')]) {
      expect(entrance(el).className).toContain('animate-slide');
      expect(entrance(el).className).not.toContain('animate-enter');
    }
    for (const el of [screen.getByText('Go'), screen.getByText('aside')]) expect(entrance(el).className).toContain('animate-enter');
    // The staged entrance still ends inside the 1.2s cap: the stagger index stops at 4 (Hero) and at 8 (styles.css).
    expect(entrance(screen.getByText('aside')).style.getPropertyValue('--ejm-i')).toBe('4');
  });

  it('defines the slide entrance without opacity, and keeps the 1.12s cap and the reduced-motion guard', () => {
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'base.css'), 'utf8');
    expect(css).toMatch(/@keyframes ejm-slide \{ from \{ transform: [^}]*\} \}/);
    expect(css.match(/@keyframes ejm-slide[^\n]*/)![0]).not.toContain('opacity');
    const utility = css.match(/@utility animate-slide \{[\s\S]*?\n\}/)![0];
    expect(utility).toContain('@media (prefers-reduced-motion: no-preference)');
    expect(utility).toContain('min(var(--ejm-i, 0), 8)');
  });
});

describe.each(themes)('accessibility in %s', theme => {
  it('layout parts pass axe', async () => {
    const { container } = renderIn(
      theme,
      <main>
        <Hero title="Title" actions={<a href="#x">Action</a>}><p>Lead</p></Hero>
        <Band tone="sage">
          <SectionHead kicker="01 / Part" title="A heading"><p>Copy</p></SectionHead>
          <Rows aria-label="List"><Row title="One" href="#one" description="First" meta="mono" /><Row title="Two" state="disabled" /></Rows>
          <Facts><Fact label="license">MIT</Fact></Facts>
          <Stat figure="8" label="requests" />
          <Rule label="Part" />
          <p><Text variant="code">code</Text> <Status status="warn" /></p>
        </Band>
        <ProductStage><ProductWindow title="ejm-cli">x</ProductWindow></ProductStage>
        <Page variant="application"><SectionHead as="h2" purpose="application" title="Activity"><p>Recent changes.</p></SectionHead></Page>
      </main>,
    );
    await expectAccessible(container);
  });
});
