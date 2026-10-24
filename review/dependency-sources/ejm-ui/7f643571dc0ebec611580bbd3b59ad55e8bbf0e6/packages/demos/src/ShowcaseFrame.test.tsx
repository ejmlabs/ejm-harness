import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { ShowcaseFrame } from './ShowcaseFrame';

afterEach(cleanup);
it('gives each product its own note association and retains native playback controls', () => {
  render(<>
    <ShowcaseFrame label="CLI example"><button>Pause CLI</button></ShowcaseFrame>
    <ShowcaseFrame label="IDE example" note="Recorded local output."><button>Restart IDE</button></ShowcaseFrame>
  </>);
  const cli = screen.getByRole('group', { name: 'CLI example' });
  const ide = screen.getByRole('group', { name: 'IDE example' });
  expect(cli).toHaveAccessibleDescription('Example session. Runs in your browser without a model connection.');
  expect(ide).toHaveAccessibleDescription('Recorded local output.');
  expect(cli.getAttribute('aria-describedby')).not.toBe(ide.getAttribute('aria-describedby'));
  expect(screen.getByRole('button', { name: 'Pause CLI' })).toBeEnabled();
  expect(screen.getByRole('button', { name: 'Restart IDE' })).toBeEnabled();
});
