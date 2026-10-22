import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { expectAccessible, renderIn, themes } from '../test-setup';
import { RadioGroup } from './radio-group';

const options = [
  { value: 'fast', label: 'Fast', description: 'A short session.' },
  { value: 'unavailable', label: 'Unavailable', disabled: true },
  { value: 'long', label: 'Long' },
];

function Choice({ onChange = () => {} }: { onChange?: (value: string) => void }) {
  const [value, setValue] = useState('');
  return <form aria-label="Session"><RadioGroup label="Duration" name="duration" value={value} required options={options} onValueChange={next => { setValue(next); onChange(next); }} /></form>;
}

describe('RadioGroup', () => {
  it('keeps native validation, labels, descriptions and submitted values with controlled selection', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Choice onChange={onChange} />);
    const form = screen.getByRole('form', { name: 'Session' }) as HTMLFormElement;
    expect(form.checkValidity()).toBe(false);
    expect(screen.getByRole('radio', { name: 'Fast' })).toHaveAccessibleDescription('A short session.');
    expect(screen.getByRole('radio', { name: 'Unavailable' })).toBeDisabled();
    await user.click(screen.getByText('A short session.'));
    expect(screen.getByRole('radio', { name: 'Fast' })).toBeChecked();
    expect(form.checkValidity()).toBe(true);
    expect(new FormData(form).get('duration')).toBe('fast');
    expect(onChange).toHaveBeenLastCalledWith('fast');
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('radio', { name: 'Long' })).toBeChecked();
    expect(new FormData(form).get('duration')).toBe('long');
    expect(onChange).toHaveBeenLastCalledWith('long');
  });

  it('disables the whole group without submitting its selected value', () => {
    render(<form aria-label="Session"><RadioGroup label="Duration" name="duration" value="fast" disabled required options={options} onValueChange={() => {}} /></form>);
    const form = screen.getByRole('form', { name: 'Session' }) as HTMLFormElement;
    for (const radio of screen.getAllByRole('radio')) expect(radio).toBeDisabled();
    expect(form.checkValidity()).toBe(true);
    expect(new FormData(form).has('duration')).toBe(false);
  });

  it.each(themes)('names the group and passes accessibility checks in %s', async theme => {
    const { container } = renderIn(theme, <RadioGroup label="Duration" name="duration" value="fast" options={options} onValueChange={() => {}} density="compact" />);
    expect(screen.getByRole('radiogroup', { name: 'Duration' })).toBeInTheDocument();
    await expectAccessible(container);
  });
});
