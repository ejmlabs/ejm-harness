import { describe, expect, it } from 'vitest';
import { parseDoc, slug, splitDoc } from './docs.mjs';

const blocks = (source: string) => parseDoc(`## Guide\n\n${source}`).sections[0]!.blocks;

describe('compiled documentation contracts', () => {
  it('preserves literal entities in code spans, headings and table headers', () => {
    const page = parseDoc('## Use `&lt;tag&gt;`\n\nOutside &amp; inside `&lt;tag&gt; &amp; &#60;`.\n\n| `&lt;tag&gt;` |\n| --- |\n| `&amp;` |');
    expect(page.sections[0]!.title).toBe('Use &lt;tag&gt;');
    expect(page.sections[0]!.blocks).toEqual([
      { type: 'p', children: ['Outside & inside ', ['code', '&lt;tag&gt; &amp; &#60;'], '.'] },
      { type: 'table', caption: 'Use &lt;tag&gt;', heads: ['&lt;tag&gt;'], rows: [[[['code', '&amp;']]]] },
    ]);
  });

  it('preserves nested lists, ordered starts, code labels, callouts, and every table cell', () => {
    const page = parseDoc([
      '---', 'title: Guide', 'lead: Use it.', 'description: A guide.', 'group: Start', '---', '',
      '## Build {#build}', '',
      '3. Use **source** and [reference](/docs/).', '   - Inspect `lib.rs`.', '   - Keep the next step.', '',
      '> **Warning:** Save first.', '>', '> Then run the command.', '',
      '```sh title="Run it"', 'npm test', '```', '',
      'Table: Support', '', '| OS | CPU | Build | Notes |', '| --- | --- | --- | --- |', '| Linux | x64 | `cargo` | **Ready** |',
    ].join('\n'), { requireFrontMatter: true, requireExplicitIds: true, requiredMetadata: ['title', 'lead', 'description', 'group'] });
    expect(page).toMatchObject({ title: 'Guide', lead: 'Use it.', sections: [{ id: 'build' }] });
    expect(page.sections[0]!.blocks[0]).toMatchObject({ type: 'list', ordered: true, start: 3, items: [[{ type: 'p' }, { type: 'list', items: [[{ type: 'p', children: ['Inspect ', ['code', 'lib.rs'], '.'] }], [{ type: 'p' }]] }]] });
    expect(page.sections[0]!.blocks[1]).toEqual({ type: 'callout', kind: 'warning', blocks: [{ type: 'p', children: ['Save first.'] }, { type: 'p', children: ['Then run the command.'] }] });
    expect(page.sections[0]!.blocks[2]).toEqual({ type: 'code', lang: 'sh', label: 'Run it', code: 'npm test' });
    expect(page.sections[0]!.blocks[3]).toEqual({ type: 'table', caption: 'Support', heads: ['OS', 'CPU', 'Build', 'Notes'], rows: [[['Linux'], ['x64'], [['code', 'cargo']], [['strong', ['Ready']]]]] });
  });

  it('keeps code tabs and only sends interactive content to the browser', () => {
    const page = parseDoc([
      '## Install', '', 'A long prose sentence.', '',
      '<!-- tabs: Package manager -->', '',
      '```sh tab="npm" label="Install with npm"', 'npm install', '```', '',
      '```sh tab="pnpm"', 'pnpm install', '```', '',
      '<!-- /tabs -->', '', '- A nested command:', '', '  ```sh', '  npm test', '  ```',
    ].join('\n'));
    const { full, client, requiresRich } = splitDoc(page);
    expect(full.sections[0]!.items[0]).toMatchObject({ type: 'prose', blocks: [{ type: 'p', children: ['A long prose sentence.'] }] });
    expect(client.sections[0]!.items[0]).toEqual({ type: 'prose' });
    expect(client.sections[0]!.items[1]).toMatchObject({ type: 'tabs', label: 'Package manager', tabs: [{ label: 'npm', codeLabel: 'Install with npm' }, { label: 'pnpm' }] });
    expect(client.sections[0]!.items[2]).toMatchObject({ type: 'list', items: [[{ type: 'p' }, { type: 'code', code: 'npm test' }]] });
    expect(JSON.stringify(client)).not.toContain('A long prose sentence.');
    expect(requiresRich).toBe(true);
    expect(splitDoc(parseDoc('## A\n\n```sh\ntrue\n```')).requiresRich).toBe(false);
  });

  it('retains explicit anchors and tracks GitHub-compatible duplicate heading ids', () => {
    const page = parseDoc('## Install {#install}\n\n### Install\n\n## Install\n\n### A.B & C!\n');
    expect(page.sections.map((section) => section.id)).toEqual(['install', 'install-2']);
    expect(page.sections[0]!.blocks[0]).toEqual({ type: 'h3', id: 'install-1', title: 'Install' });
    expect(slug('A.B & C!')).toBe('ab--c');
    expect(() => parseDoc('## A {#same}\n\n## B {#same}')).toThrow(/duplicate heading id/);
    expect(() => parseDoc('## No ID', { requireExplicitIds: true })).toThrow(/explicit/);
  });

  it('checks front matter and refuses unsupported syntax without dropping content', () => {
    expect(() => parseDoc('## A', { requireFrontMatter: true })).toThrow(/front matter/);
    expect(() => parseDoc('---\ntitle: T\n---\n## A', { requiredMetadata: ['lead'] })).toThrow(/needs lead/);
    expect(() => parseDoc('---\ntitle: T\ntitle: Duplicate\n---\n## A')).toThrow(/front matter/);
    expect(() => parseDoc('---\norder: invalid\n---\n## A')).toThrow(/order must be a number/);
    expect(() => blocks('<div>raw HTML</div>')).toThrow(/HTML blocks/);
    expect(() => blocks('![Image](image.png)')).toThrow(/not supported/);
    expect(() => blocks('<!-- tabs: Language -->\n\n```sh tab="sh"\ntrue\n```')).toThrow(/unclosed code tabs/);
  });

  it('resolves repository links through consumer policy and rejects executable URLs on both sides', () => {
    const page = parseDoc('## A\n\n[Guide][guide] and a&amp;b.\n\n[guide]: reference.md#use\n', { link: (href) => ({ href: `/docs/${href.replace('.md', '/')}`, external: false }) });
    expect(page.sections[0]!.blocks).toEqual([{ type: 'p', children: [['a', '/docs/reference/#use', false, ['Guide']], ' and a&b.'] }]);
    expect(blocks('[Web](//example.com/docs) and [mail](mailto:hello@example.com) and [local](#build)')).toEqual([{ type: 'p', children: [['a', '//example.com/docs', true, ['Web']], ' and ', ['a', 'mailto:hello@example.com', false, ['mail']], ' and ', ['a', '#build', false, ['local']]] }]);
    const quote = parseDoc('## A\n\n> [!NOTE]\n> Read [guide].\n\n[guide]: /docs/');
    expect(quote.sections[0]!.blocks[0]).toMatchObject({ type: 'callout', blocks: [{ type: 'p', children: ['Read ', ['a', '/docs/', false, ['guide']], '.'] }] });
    expect(() => blocks('[Bad](javascript:alert(1))')).toThrow(/unsafe link/);
    expect(() => blocks('[Bad](jav&#x61;script:alert(1))')).toThrow(/unsafe link/);
    expect(() => parseDoc('## A\n\n[Bad](/docs/)', { link: () => ({ href: 'data:text/html,bad', external: true }) })).toThrow(/unsafe link/);
  });
});
