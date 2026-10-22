import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { expectAccessible, renderIn, themes } from '../test-setup';
import { Button } from './button';
import { ToastProvider, useToast, type ToastInput } from './toast';

function Fire({ input }: { input: ToastInput }) {
  const { toast } = useToast();
  return <Button onClick={() => toast(input)}>Show</Button>;
}
const ui = (input: ToastInput, duration?: number) => (
  <ToastProvider duration={duration}>
    <Fire input={input} />
  </ToastProvider>
);

describe('Toast', () => {
  it('announces in a polite live region without taking focus', async () => {
    const user = userEvent.setup();
    render(ui({ title: 'Saved', description: 'Your changes are live.' }));
    const trigger = screen.getByRole('button', { name: 'Show' });
    await user.click(trigger);
    expect(await screen.findAllByText('Saved')).not.toHaveLength(0);
    // Radix announces through a visually hidden region; "background" toasts use aria-live=polite.
    const live = document.body.querySelector('[aria-live="polite"]');
    expect(live).not.toBeNull();
    await waitFor(() => expect(live).toHaveTextContent('Saved'));
    expect(document.body.querySelector('[aria-live="assertive"]')).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it('dismisses itself after its duration', async () => {
    vi.useFakeTimers();
    try {
      render(ui({ title: 'Copied', duration: 60 }));
      fireEvent.click(screen.getByRole('button', { name: 'Show' }));
      expect(screen.getAllByText('Copied').length).toBeGreaterThan(0);
      await act(async () => { await vi.advanceTimersByTimeAsync(59); });
      expect(screen.getAllByText('Copied').length).toBeGreaterThan(0);
      await act(async () => { await vi.advanceTimersByTimeAsync(1); });
      expect(screen.queryAllByText('Copied')).toHaveLength(0);
    } finally { vi.useRealTimers(); }
  });

  it('dismisses with the Dismiss button', async () => {
    const user = userEvent.setup();
    render(ui({ title: 'Copied' }));
    await user.click(screen.getByRole('button', { name: 'Show' }));
    await user.click(await screen.findByRole('button', { name: 'Dismiss' }));
    await waitFor(() => expect(screen.queryAllByText('Copied')).toHaveLength(0), { timeout: 2000 });
  });

  it('runs its action', async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(ui({ title: 'Deleted', action: { label: 'Undo', altText: 'Undo the delete', onClick } }));
    await user.click(screen.getByRole('button', { name: 'Show' }));
    await user.click(await screen.findByRole('button', { name: 'Undo' }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it.each(themes)('passes the accessibility check in %s', async theme => {
    const user = userEvent.setup();
    const { container } = renderIn(theme, ui({ title: 'Saved', description: 'Done.' }));
    await user.click(screen.getByRole('button', { name: 'Show' }));
    await screen.findAllByText('Saved');
    await expectAccessible(container);
  });
});
