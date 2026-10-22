import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { LazyPreview } from './LazyPreview';

afterEach(cleanup);
it('retains readable server content and announces a rejected player module', async () => {
  const load = vi.fn(async () => { throw new Error('Unavailable test chunk'); });
  render(<LazyPreview server={() => <p>Retained recorded outcome</p>} load={load} eager />);
  expect(screen.getByText('Retained recorded outcome')).toBeVisible();
  expect(await screen.findByRole('status')).toHaveTextContent('Playback is unavailable');
  expect(screen.getByText('Retained recorded outcome')).toBeVisible();
  expect(load).toHaveBeenCalledOnce();
});
