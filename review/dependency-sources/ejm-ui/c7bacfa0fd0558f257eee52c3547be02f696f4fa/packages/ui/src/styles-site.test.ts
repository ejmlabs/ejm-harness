// @vitest-environment node
import { compile } from '@tailwindcss/node';
import { Scanner } from '@tailwindcss/oxide';
import { fileURLToPath } from 'node:url';
import postcss from 'postcss';
import { expect, it } from 'vitest';

it.each(['styles.css', 'ide.css'])('ships responsive product windows through the public %s profile', async profile => {
  const compiler = await compile(`@import "@ejm/ui/styles-site.css" source(none);\n@import "@ejm/demos/${profile}";`, {
    base: fileURLToPath(new URL('./', import.meta.url)),
    onDependency() {},
  });
  // Use only the public profiles' sources. Supplied class names would hide a missing owner source.
  const css = postcss.parse(compiler.build(new Scanner({ sources: compiler.sources }).scan()));
  const rules: { selector: string; query?: string; declarations: Record<string, string> }[] = [];
  css.walkRules(rule => {
    const declarations: Record<string, string> = {};
    rule.walkDecls(declaration => { declarations[declaration.prop] = declaration.value; });
    const parent = rule.parent;
    rules.push({
      selector: rule.selector,
      query: parent?.type === 'atrule' && parent.name === 'container' ? parent.params : undefined,
      declarations,
    });
  });
  const emitted = (selector: string, query: string | undefined, declarations: Record<string, string>) => {
    expect(rules.filter(rule => rule.selector === selector), selector).toContainEqual({
      selector, query, declarations: expect.objectContaining(declarations),
    });
  };
  // Windows tuck into a stack on narrow stages and occupy fixed slots on wider stages.
  emitted('.ms-\\[6\\%\\]', undefined, { 'margin-inline-start': '6%' });
  emitted('.\\@md\\:absolute', '(width >= 28rem)', { position: 'absolute' });
  emitted('.\\@md\\:w-\\[66\\%\\]', '(width >= 28rem)', { width: '66%' });
  // Window labels and type follow the stage width rather than the page viewport.
  emitted('.\\@max-md\\:hidden', '(width < 28rem)', { display: 'none' });
  emitted('.\\@md\\:\\[--ejm-window-text\\:clamp\\(0\\.8125rem\\,1\\.9cqw\\,1\\.75rem\\)\\]', '(width >= 28rem)', {
    '--ejm-window-text': 'clamp(0.8125rem, 1.9cqw, 1.75rem)',
  });
});
