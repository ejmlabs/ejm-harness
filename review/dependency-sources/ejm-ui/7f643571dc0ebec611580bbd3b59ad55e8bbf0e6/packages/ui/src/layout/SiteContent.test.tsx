import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { createRef } from 'react';
import { afterEach, expect, it } from 'vitest';
import { LinkProvider, type LinkProps } from '../shell/link';
import { ContentFlow, FileDetail, FocusRegion, InlineLink, Note, VisuallyHidden } from './SiteContent';

afterEach(cleanup);

it('keeps routing, downloads and external link disclosure in shared inline links', () => {
  const Router = (props: LinkProps) => <a {...props} data-route="site" />;
  render(<LinkProvider value={Router}><ContentFlow>
    <InlineLink href="/guide/">Guide</InlineLink>
    <InlineLink href="/kit.zip" download>Kit</InlineLink>
    <InlineLink href="https://example.test" external>Source</InlineLink>
  </ContentFlow></LinkProvider>);
  expect(screen.getByRole('link', { name: 'Guide' })).toHaveAttribute('data-route', 'site');
  expect(screen.getByRole('link', { name: 'Kit' })).toHaveAttribute('download', '');
  const external = screen.getByRole('link', { name: 'Source, opens another site' });
  expect(external).not.toHaveAttribute('data-route');
  expect(external).toHaveAttribute('rel', 'external noopener');
});

it('preserves release announcements, accessible download names and programmatic focus', () => {
  const region = createRef<HTMLDivElement>();
  render(<FocusRegion ref={region} tabIndex={-1} aria-label="Installers">
    <ContentFlow role="status" aria-live="polite"><p>Checking installers</p></ContentFlow>
    <InlineLink href="/app.zip">Download{' '}<VisuallyHidden>for macOS</VisuallyHidden></InlineLink>
    <FileDetail>SHA-256 1234</FileDetail>
    <Note id="release-note">Builds are checked before publication.</Note>
  </FocusRegion>);
  expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite');
  expect(screen.getByRole('link', { name: 'Download for macOS' })).toHaveAttribute('href', '/app.zip');
  expect(screen.getByText('SHA-256 1234')).toBeInTheDocument();
  region.current?.focus();
  expect(region.current).toHaveFocus();
  expect(document.getElementById('release-note')).toHaveTextContent('Builds are checked before publication.');
});
