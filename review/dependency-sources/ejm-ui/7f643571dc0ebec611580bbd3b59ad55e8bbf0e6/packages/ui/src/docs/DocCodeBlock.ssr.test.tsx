// @vitest-environment node
import { renderToString } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { DocCodeTabs } from './data';

const block: DocCodeTabs = {
  type: 'tabs',
  label: 'Install',
  tabs: [
    { label: 'macOS', lang: 'sh', code: 'brew install tool', codeLabel: 'macOS setup' },
    { label: 'Linux', lang: 'sh', code: 'apt install tool', codeLabel: 'Linux setup' },
  ],
};

afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

it('cold server rendering without an SSR build flag includes every tab and preserves the code renderer', async () => {
  vi.stubEnv('SSR', undefined);
  const { DocCodeBlock } = await import('./DocCodeBlock');
  const html = renderToString(<DocCodeBlock block={block} renderCode={(code) => <pre data-code-label={code.label}>{code.code}</pre>} />);
  for (const tab of block.tabs) {
    expect(html).toContain(tab.label);
    expect(html).toContain(tab.code);
    expect(html).toContain(`data-code-label="${tab.codeLabel}"`);
  }
  expect(html).not.toContain('<template');
  expect(html).not.toContain('role="tablist"');
  expect(html).not.toContain('hidden');
});
