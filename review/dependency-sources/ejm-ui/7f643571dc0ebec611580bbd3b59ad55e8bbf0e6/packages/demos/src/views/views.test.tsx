import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { expectAccessible, renderIn, themes } from '../test-helpers';
import { CliView, HarnessView, IdeView } from '.';

describe('static product views', () => {
  it('ejm-cli shows the blocked patch as a diff, read-only grants and the simulated label', () => {
    render(<CliView label="ejm-cli" />);
    expect(screen.getByText(/grants: read$/)).toBeInTheDocument();
    expect(screen.getByText(/^-pub fn window_open/)).toBeInTheDocument();
    expect(screen.getByText(/^\+pub fn window_open/)).toBeInTheDocument();
    expect(screen.getByText(/patch_file src\/retry\.rs/)).toBeInTheDocument();
    expect(screen.getByText(/blocked: write access is not granted/)).toBeInTheDocument();
    expect(screen.getAllByText('Simulated screen').length).toBeGreaterThan(0);
  });

  it('ejm-ide shows the tool calls with the names ejm-ide documents, the diff and the edited code', () => {
    render(<IdeView label="ejm-ide" layout="split" />);
    for (const tool of ['workbench_read_file', 'workbench_search_code', 'workbench_write_file']) expect(screen.getByText(tool)).toBeInTheDocument();
    expect(screen.getByRole('group').querySelectorAll('[data-changed]').length).toBeGreaterThan(0);
    expect(screen.getAllByText('applied', { exact: false }).length).toBeGreaterThan(0);
  });

  it('ejm-harness shows the governor, sessions recovering and the journal', () => {
    render(<HarnessView label="ejm-harness" layout="split" />);
    expect(screen.getByText('sample values')).toBeInTheDocument();
    expect(screen.getAllByText('recovering').length).toBeGreaterThan(0);
    expect(screen.getByText('worker.crashed')).toBeInTheDocument();
  });

  it.each(themes)('pass accessibility checks in the %s theme', async (theme) => {
    const { container } = renderIn(theme, <><CliView label="ejm-cli" /><IdeView label="ejm-ide" layout="split" /><HarnessView label="ejm-harness" layout="split" /></>);
    await expectAccessible(container);
  });
});
