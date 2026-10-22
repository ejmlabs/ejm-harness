// @vitest-environment node
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { compile, optimize } from '@tailwindcss/node';
import postcss from 'postcss';
import { expect, it } from 'vitest';
import { AppGroup } from './Application';

it('keeps emitted native group layout and wrapping equal to React AppGroup', async () => {
  const base = fileURLToPath(new URL('../', import.meta.url));
  const compiler = await compile('@import "./base.css";', { base, onDependency: () => {} });
  const element = AppGroup({});
  const react = postcss.parse(optimize(compiler.build(element.props.className.split(/\s+/)), { minify: true }).code);
  const expected: Record<string, string> = {};
  react.walkAtRules('layer', layer => {
    if (layer.params === 'utilities') layer.walkDecls(declaration => { expected[declaration.prop] = declaration.value; });
  });
  const native = postcss.parse(await readFile(new URL('../../public/static.css', import.meta.url), 'utf8'));
  const actual: Record<string, string> = {};
  native.walkRules(rule => {
    if (rule.selectors.includes('[data-ejm=group]')) rule.walkDecls(declaration => { actual[declaration.prop] = declaration.value; });
  });
  expect(expected).toMatchObject({ display: 'grid', 'min-width': '0', 'overflow-wrap': 'anywhere' });
  expect(expected).toHaveProperty('gap');
  expect(expected).toHaveProperty('grid-template-columns');
  expect(actual).toEqual(expected);
});
