import { clsx, type ClassValue } from 'clsx';
import { createTailwindMerge } from 'tailwind-merge';
import { slimConfig } from './merge-config';

// tailwind-merge must know the ejm-ui type scale (THEME.md): without it, `text-meta` and a color such as `text-muted` look like two
// colors and one of them is dropped. The config is tailwind-merge's own, cut to the class groups a component or page overrides
// (scripts/build-merge-config.mjs); a class outside those groups is kept as written.
const twMerge = createTailwindMerge(() => {
  const config = slimConfig();
  // The radius names of THEME.md, so `rounded-pill`, `rounded-control` and `rounded-compact` replace each other like any other radius.
  (config.theme.radius as unknown[]).push('control', 'pill', 'window', 'compact');
  (config.theme.spacing as unknown[]).push('field', 'content');
  (config.classGroups['font-size'] as unknown[]).push({ text: ['display-lg', 'display', 'title', 'heading', 'lead', 'stat', 'body', 'meta'] });
  return config;
});

/** Join class names and resolve Tailwind conflicts (the shadcn/ui helper). */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
