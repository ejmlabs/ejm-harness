import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import IdeDemo from './ide';
import { expectAccessible, renderIn, setup, themes } from '../test-helpers';

const fix = () => screen.getByRole('button', { name: 'Fix the flaky retry test' });
const chat = () => screen.getByRole('log', { name: 'Edi messages' });

describe('IdeDemo', () => {
  it('says it is simulated and labels Accept and Reject as showing both outcomes', async () => {
    const { options, settle } = setup();
    const user = userEvent.setup();
    renderIn('light', <IdeDemo {...options} />);
    expect(screen.getByText('Simulated')).toBeVisible();
    await user.click(fix());
    await settle();
    expect(screen.getByRole('group', { name: 'Review the proposed change' })).toBeInTheDocument();
    expect(screen.getByText(/Demo only: the real ejm-ide checks each proposed write automatically/)).toBeVisible();
  });

  it('accepting the proposed write applies it, marks the changed lines and removes the buttons', async () => {
    const { options, settle } = setup();
    const user = userEvent.setup();
    renderIn('light', <IdeDemo {...options} />);
    await user.click(fix());
    await settle();
    expect(within(chat()).getByText('workbench_write_file')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Accept' }));
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    expect(within(chat()).getByText(/applied/)).toBeInTheDocument();
    const panel = screen.getByRole('tabpanel');
    expect(panel.querySelectorAll('[data-changed]').length).toBeGreaterThan(0);
    expect(panel).toHaveTextContent('now: Instant');
  });

  it('rejecting leaves the file unchanged', async () => {
    const { options, settle } = setup();
    const user = userEvent.setup();
    renderIn('light', <IdeDemo {...options} />);
    await user.click(fix());
    await settle();
    await user.click(screen.getByRole('button', { name: 'Reject' }));
    expect(within(chat()).getByText(/rejected/)).toBeInTheDocument();
    expect(screen.getByRole('tabpanel').querySelectorAll('[data-changed]')).toHaveLength(0);
  });

  it('opens the palette from a button, filters, runs a command, and Escape returns focus to the button', async () => {
    const { options } = setup();
    const user = userEvent.setup();
    const { container } = renderIn('light', <IdeDemo {...options} />);
    const button = screen.getByRole('button', { name: 'Commands' });
    await user.click(button);
    const box = screen.getByRole('combobox', { name: 'Type a command' });
    expect(box).toHaveFocus();
    await user.type(box, 'toggle{Enter}');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(container.querySelector('[data-theme="dark"]')).not.toBeNull();
    await user.click(button);
    await user.keyboard('{Escape}');
    expect(button).toHaveFocus();
  });

  it('moves between tabs with the arrow keys and closes one with Delete', async () => {
    const { options } = setup();
    const user = userEvent.setup();
    renderIn('light', <IdeDemo {...options} />);
    await user.click(screen.getByRole('button', { name: 'lib.rs' }));
    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(2);
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('lib.rs');
    screen.getByRole('tab', { selected: true }).focus();
    await user.keyboard('{ArrowLeft}');
    expect(screen.getByRole('tab', { selected: true })).toHaveTextContent('retry.rs');
    await user.keyboard('{Delete}');
    expect(screen.getAllByRole('tab')).toHaveLength(1);
  });

  it.each(themes)('has no accessibility violations in the %s theme', async (theme) => {
    const { options, settle } = setup();
    const user = userEvent.setup();
    const { container } = renderIn(theme, <IdeDemo {...options} />);
    await user.click(fix());
    await settle();
    await expectAccessible(container);
  });
});
