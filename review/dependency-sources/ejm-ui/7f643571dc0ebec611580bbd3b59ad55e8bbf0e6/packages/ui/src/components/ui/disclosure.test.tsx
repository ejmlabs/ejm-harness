import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { expectAccessible, renderIn, themes } from '../test-setup';
import { Disclosure } from './disclosure';
import { Input } from './input';

function Settings() {
  const [open, setOpen] = useState(false);
  return <Disclosure summary="Settings" open={open} onToggle={event => setOpen(event.currentTarget.open)}><Input aria-label="Nickname" defaultValue="Ada" /></Disclosure>;
}

describe('Disclosure', () => {
  it('keeps controlled native toggling and preserves mounted form state while closed', async () => {
    const user = userEvent.setup();
    const { container } = render(<Settings />);
    const details = container.querySelector('details')!;
    const field = screen.getByLabelText('Nickname');
    await user.click(screen.getByText('Settings'));
    await waitFor(() => expect(details.open).toBe(true));
    await user.clear(field);
    await user.type(field, 'Grace');
    await user.click(screen.getByText('Settings'));
    await waitFor(() => expect(details.open).toBe(false));
    expect(screen.getByLabelText('Nickname')).toBe(field);
    expect(field).toHaveValue('Grace');
    await user.click(screen.getByText('Settings'));
    await waitFor(() => expect(details.open).toBe(true));
    expect(screen.getByLabelText('Nickname')).toHaveValue('Grace');
  });

  it.each(themes)('passes accessibility checks with visible content in %s', async theme => {
    const { container } = renderIn(theme, <Disclosure summary="Settings" open><Input aria-label="Nickname" /></Disclosure>);
    await expectAccessible(container);
  });
});
