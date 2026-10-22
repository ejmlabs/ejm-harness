import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { extendTailwindMerge } from 'tailwind-merge';
import { describe, expect, it } from 'vitest';
import { cn } from './utils';

describe('cn', () => {
  it('keeps a type-scale size and a color together, but still resolves real conflicts', () => {
    expect(cn('text-meta', 'text-muted')).toBe('text-meta text-muted');
    expect(cn('text-heading text-fg', 'text-danger')).toBe('text-heading text-danger');
    expect(cn('text-title', 'text-heading')).toBe('text-heading');
    expect(cn('px-2', 'px-4')).toBe('px-4');
    // The radius names of THEME.md replace each other.
    expect(cn('rounded-pill px-4', 'rounded-compact')).toBe('px-4 rounded-compact');
    expect(cn('rounded-control', 'rounded-pill')).toBe('rounded-pill');
    expect(cn('rounded-t-window', 'rounded-t-compact')).toBe('rounded-t-compact');
  });
  it('lets named application gaps and numeric overrides replace each other', () => {
    expect(cn('grid gap-content', 'gap-field')).toBe('grid gap-field');
    expect(cn('grid gap-field', 'gap-4')).toBe('grid gap-4');
    expect(cn('gap-4', 'gap-content')).toBe('gap-content');
    expect(cn('sm:gap-content', 'sm:gap-4')).toBe('sm:gap-4');
    expect(cn('gap-content', 'gap-y-field')).toBe('gap-content gap-y-field');
  });
});

// The slim config (scripts/build-merge-config.mjs) must give what the full tailwind-merge gives for every class the sources use.
describe('cn against the full tailwind-merge', () => {
  const full = extendTailwindMerge({
    extend: {
      theme: { radius: ['control', 'pill', 'window', 'compact'], spacing: ['field', 'content'] },
      classGroups: { 'font-size': [{ text: ['display-lg', 'display', 'title', 'heading', 'lead', 'stat', 'body', 'meta'] }] },
    },
  });
  const here = dirname(fileURLToPath(import.meta.url));
  const files = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap(e => (e.isDirectory() ? files(join(dir, e.name)) : /\.tsx?$/.test(e.name) && !/\.test\.|test-setup|merge-config/.test(e.name) ? [join(dir, e.name)] : []));
  // Every whitespace-separated word of every string literal under src that looks like a class. Words that are not classes are
  // harmless: both merges keep them.
  const scanned = files(join(here, '..')).flatMap(f =>
    [...readFileSync(f, 'utf8').matchAll(/(['"`])((?:(?!\1)[^\\\n]|\\.){2,400})\1/g)].flatMap(m => (m[2] ?? '').split(/\s+/)),
  ).filter(t => /^[!a-z@[][a-z0-9@[\]:/._()%&>*=#,!'"-]*$/.test(t) && /[a-z]/.test(t) && t.length < 90);
  // What a page or a consumer typically passes as className to override a component.
  const overrides = [
    'p-0', 'p-4', 'px-2', 'px-6', 'py-3', 'pt-0', 'pl-2', 'pe-3', 'm-0', 'mx-auto', 'mt-8', 'mb-0', 'ml-2', 'w-full', 'w-[12rem]', 'h-10', 'size-6', 'min-h-0', 'max-w-measure', 'max-w-prose',
    'text-sm', 'text-lg', 'text-title', 'text-meta', 'text-fg', 'text-muted', 'text-[#fff]', 'text-center', 'font-bold', 'font-mono', 'leading-6', 'tracking-wide', 'underline', 'no-underline',
    'bg-tone', 'bg-primary', 'bg-transparent', 'rounded-none', 'rounded-pill', 'rounded-compact', 'rounded-control', 'rounded-t-window', 'rounded-t-lg', 'border', 'border-0', 'border-t', 'border-line', 'border-b-2', 'divide-y', 'shadow-none', 'shadow-lg', 'opacity-50', 'ring-2',
    'flex', 'hidden', 'block', 'grid', 'inline-flex', 'flex-col', 'flex-row', 'items-center', 'items-start', 'justify-between', 'gap-2', 'gap-content', 'gap-field', 'gap-x-4', 'gap-y-1', 'grow', 'shrink-0', 'basis-1/2',
    'absolute', 'relative', 'fixed', 'sticky', 'inset-0', 'top-0', 'z-10', 'overflow-hidden', 'overflow-x-auto', 'whitespace-nowrap', 'whitespace-normal', 'truncate', 'sr-only', 'not-sr-only',
    'transition', 'duration-200', 'ease-in', 'animate-none', 'cursor-pointer', 'pointer-events-none', 'select-none', 'fill-current', 'stroke-2', 'translate-x-1', 'scale-95', 'rotate-45',
    'hover:bg-tone', 'hover:p-2', 'md:px-8', 'md:hidden', 'dark:text-fg', '!p-2', 'p-[3px]', 'px-(--gutter)', 'sm:flex', 'focus-visible:outline-2', 'before:p-2',
  ];
  const tokens = [...new Set([...scanned, ...overrides])];
  const pairs: [string, string][] = tokens.flatMap(a => tokens.map((b): [string, string] => [a, b]));

  it('agrees on every pair of classes found in the sources and on typical overrides', () => {
    const wrong = pairs.filter(([a, b]) => cn(a, b) !== full(`${a} ${b}`)).slice(0, 8);
    expect(wrong.map(([a, b]) => `${a} + ${b}: ${cn(a, b)} vs ${full(`${a} ${b}`)}`)).toEqual([]);
  }, 60_000);

  it.each([
    'leading-8 text-lg/6', 'text-lg/6 leading-8',
    '@container/name @container-normal', '@container-normal @container/name',
    'border-t-2 border-t-line', 'border-t-line border-t-2',
    'border-e-2 border-e-line', 'border-e-line border-e-2',
    'border-b-2 border-b-line', 'border-b-line border-b-2',
  ])('preserves postfix and overlapping validator behavior for %s', input => {
    expect(cn(input)).toBe(full(input));
  });

  it('never drops a class the full merge keeps, for utilities whose group was cut from the config', () => {
    // These share a first word with kept groups (bg-color, text-color, border-color...), the case where a cut config goes wrong.
    const cut = [
      'bg-fixed', 'bg-cover', 'bg-center', 'bg-no-repeat', 'bg-clip-text', 'bg-origin-border', 'bg-linear-to-r', 'bg-blend-multiply', 'text-balance', 'text-pretty', 'text-ellipsis', 'text-shadow-md',
      'border-collapse', 'border-spacing-2', 'border-separate', 'outline-hidden', 'ring-offset-2', 'ring-inset', 'shadow-inner', 'inset-shadow-sm', 'decoration-wavy', 'decoration-2',
      'divide-solid', 'divide-x-reverse', 'stroke-none', 'fill-none', 'font-stretch-condensed', 'font-features-[smcp]', 'mix-blend-multiply', 'from-red-500', 'via-red-500', 'to-red-500',
      'accent-auto', 'caret-transparent', 'tabular-nums', 'slashed-zero', 'drop-shadow-md', 'blur-sm', 'backdrop-blur-sm', 'scroll-mt-4', 'snap-x', 'touch-none', 'will-change-transform', 'mask-linear-to-b',
    ];
    const bad: string[] = [];
    for (const a of cut) for (const b of [...overrides, 'bg-tone', 'text-fg', 'border-line', 'outline-focus', 'ring-focus', 'shadow-float', 'fill-current', 'stroke-current']) {
      for (const input of [`${a} ${b}`, `${b} ${a}`]) {
        const kept = new Set(full(input).split(' '));
        const got = new Set(cn(input).split(' '));
        if (![...kept].every(c => got.has(c))) bad.push(`${input} -> ${cn(input)} (full: ${full(input)})`);
      }
    }
    expect(bad.slice(0, 8)).toEqual([]);
  });
});

