import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { Brand } from './Brand';

afterEach(cleanup);

it('leaves one accessible home-link name and preserves artwork proportions across theme variants', () => {
  const { container, rerender } = render(<a href="/" aria-label="ejmlabs home"><Brand src="/ink.svg" darkSrc="/paper.svg" width={31} height={28} name="ejmlabs" /></a>);
  expect(screen.getByRole('link', { name: 'ejmlabs home' })).not.toBeNull();
  expect(screen.queryByRole('img')).toBeNull();
  expect(container.querySelectorAll('img')).toHaveLength(2);
  for (const image of container.querySelectorAll('img')) {
    expect(image.getAttribute('width')).toBe('31');
    expect(image.getAttribute('height')).toBe('28');
  }
  rerender(<Brand src="/ink.svg" darkSrc="/ink.svg" width={32} height={32} name="Project" />);
  expect(container.querySelectorAll('img')).toHaveLength(1);
});

it('uses the light artwork on the permanently dark footer in either page theme', () => {
  const { container } = render(<Brand src="/ink.svg" darkSrc="/paper.svg" width={32} height={32} name="Project" placement="footer" />);
  expect(container.querySelectorAll('img')).toHaveLength(1);
  expect(container.querySelector('img')).toHaveAttribute('src', '/paper.svg');
  expect(screen.getByText('Project')).toBeInTheDocument();
});
