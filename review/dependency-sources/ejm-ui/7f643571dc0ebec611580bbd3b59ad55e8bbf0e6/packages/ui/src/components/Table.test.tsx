import { contract } from '@ejm/core';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { expectAccessible, renderIn, themes } from './test-setup';
import { Table, tableWords } from './Table';

const columns = [
  { key: 'name', label: 'Package' },
  { key: 'size', label: 'Size (KB)' },
  { key: 'note', label: 'Note', width: 12 },
];
const rows = [
  { name: '@ejm/core', size: 12, note: 'Headless logic for every target, with no framework' },
  { name: '@ejm/ui', size: 48, note: 'Components' },
];

describe('Table', () => {
  it('default: header cells, rows, numbers right aligned in tabular figures, long cells keep their full text', () => {
    render(<Table columns={columns} rows={rows} label="Packages" />);
    const region = screen.getByRole('region', { name: 'Packages' });
    expect(region).toHaveAttribute('tabindex', '0'); // so a keyboard user can scroll it
    expect(screen.getAllByRole('columnheader').map(h => h.textContent)).toEqual(['Package', 'Size (KB)', 'Note']);
    expect(screen.getByRole('columnheader', { name: 'Size (KB)' })).toHaveClass('text-right');
    expect(screen.getByRole('columnheader', { name: 'Package' })).not.toHaveClass('text-right');
    expect(screen.getByRole('table')).toHaveClass('tabular-nums');
    expect(screen.getByTitle('Headless logic for every target, with no framework')).toBeInTheDocument();
  });

  it('empty, loading and error keep the header and put one sentence under it, in the contract words', () => {
    for (const state of ['empty', 'loading', 'error'] as const) {
      const { unmount } = render(<Table columns={columns} rows={[]} state={state} />);
      expect(screen.getAllByRole('columnheader')).toHaveLength(3);
      expect(screen.getByText(tableWords[state])).toBeInTheDocument();
      unmount();
    }
  });

  it('loading draws skeleton rows in the same columns and marks the table busy', () => {
    render(<Table columns={columns} rows={[]} state="loading" />);
    expect(screen.getByRole('table')).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByRole('status')).toHaveTextContent('Loading the table.');
  });

  it('error is announced and offers its one action', () => {
    render(<Table columns={columns} rows={[]} state="error" message="The registry did not answer." action={{ label: 'Retry' }} />);
    expect(within(screen.getByRole('alert')).getByRole('button', { name: /Retry/ })).toBeInTheDocument();
    expect(screen.getByText('The registry did not answer.')).toBeInTheDocument();
  });

  it('uses the same default words as contract.json', () => {
    expect(tableWords).toEqual(contract.text.table);
  });

  it('keeps captions, row headers and interactive cells as native content', async () => {
    const user = userEvent.setup();
    let activated = false;
    render(<Table caption="Recent releases" label="Release history" rowHeader="name" columns={columns} rows={[
      { name: <a href="/core/">Core</a>, size: 12, note: <button onClick={() => { activated = true; }}>Open release</button> },
    ]} />);
    expect(screen.getByRole('table', { name: 'Recent releases' })).toBeInTheDocument();
    expect(screen.getByRole('rowheader', { name: 'Core' })).toHaveAttribute('scope', 'row');
    expect(screen.getByRole('link', { name: 'Core' })).toHaveAttribute('href', '/core/');
    await user.click(screen.getByRole('button', { name: 'Open release' }));
    expect(activated).toBe(true);
    expect(screen.getByRole('columnheader', { name: 'Size (KB)' })).toHaveClass('text-right');
  });

  it('retains focused rich cells when keyed rows reorder', () => {
    const keyedRows = [
      { id: 'core', name: <a href="/core/">Core</a>, size: 12 },
      { id: 'ui', name: <a href="/ui/">UI</a>, size: 48 },
    ];
    const { rerender } = render(<Table columns={columns} rows={keyedRows} rowKey="id" />);
    const link = screen.getByRole('link', { name: 'Core' });
    link.focus();
    rerender(<Table columns={columns} rows={[...keyedRows].reverse()} rowKey="id" />);
    expect(screen.getByRole('link', { name: 'Core' })).toBe(link);
    expect(link).toHaveFocus();
  });

  it.each(themes)('passes the accessibility check in %s', async theme => {
    const { container } = renderIn(theme, <Table columns={columns} rows={rows} caption="Package sizes" rowHeader="name" label="Packages" />);
    await expectAccessible(container);
  });
});
