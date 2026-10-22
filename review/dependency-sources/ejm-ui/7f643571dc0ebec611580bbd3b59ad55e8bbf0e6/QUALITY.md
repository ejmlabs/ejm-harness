# Quality bar

What "good enough to ship" means for every ejmlabs site, application and page built on ejm-ui. Each line is something a reviewer can check with a screenshot, a script or the keyboard. If a line cannot be checked, it does not belong here.

The references are Anthropic, Stripe and OpenAI. ejmlabs learns from how they decide, not from what they made, and never reproduces their layouts, imagery or words. The dated critique kept in this repo measures ejmlabs pages against this bar.

## How to use it

1. Build the page from `@ejm/ui` components, layouts and views (`packages/ui/THEME.md` lists every token).
2. Run `npm run check`, `npm run build`, `node scripts/size.mjs` and `node scripts/shoot.mjs <url>` in light and dark.
3. Walk the dimensions below. A page ships when every line holds, or when the exception is written down in the critique.

## Content

- Use the fewest words that keep the meaning clear. Remove text that repeats the interface or another label.
- Prefer a familiar visual, an accessible icon, or no extra element when the meaning stays clear.
- Keep labels and instructions needed for access, consent, trust, errors and recovery. Hide only decoration from assistive tools.
- Use familiar words and active verbs. State what works now, and label examples, limits and unreleased behavior honestly.

## Typography

- Only Manrope and IBM Plex Mono are used. No other family appears in computed styles.
- A page has one display line or none. Display type is at least 3 times the body size on screens 1440px and wider.
- Headings use balanced wrapping. No heading ends with a single short word on its last line at 375px or 1440px.
- Body text lines are at most 68 characters wide. Numbers in tables and facts rows use tabular figures.
- Labels, metadata and code use IBM Plex Mono. Nothing under 12px, and body text is at least 16px on phones.

## Rhythm

- Spacing comes from the Tailwind scale and the `rhythm` and `gutter` tokens. No two neighbouring sections use the same layout twice in a row.
- Sections are separated by tone or whitespace, not by rules, except hairlines in metadata and feed rows.
- Each section has one subject and one primary action at most.

## Depth

- Elevation uses the three tokens (rest, raised, float). Only product windows, menus and overlays use them.
- Cards and panels have no shadow. Tone separates them from the page.
- Sage covers about 30 percent of a page, Paper or Mist about 60, and Forest under 10. Forest never sets a large ground.

## Imagery

- Product visuals show real commands, tools and screens from the product's README. They carry a visible "Simulated" label unless they are the live demo.
- Illustration is generated from the symbol and uses brand colors, or brand colors at reduced opacity, only.
- Decorative images have empty alt text. Meaningful images have alt text that says what they show.
- No stock photography, no gradient blobs, no glass blur.

## Motion

- Motion uses transform and opacity only, under 300ms for interactions and under 1.2 seconds for a page entrance.
- With reduced motion, or with scripts off, every element is visible in its final place.
- Nothing moves without a cause: arrival, a state change or a user action.
- Text that can be the Largest Contentful Paint (the hero headline and lead) never starts at opacity 0: it slides (`animate-slide`) and does not fade, so LCP stays at FCP.

## Detail

- Every interactive element has rest, hover, focus, pressed and disabled states. Focus is visible at 3:1 against its ground.
- Lists and tables have empty, loading and error states with a sentence and, where one exists, one action.
- Links to another site use `rel="external"` and show the arrow. Copy has no em dashes.

## Responsiveness

- No horizontal overflow at any matrix size from 320px to 5120px, in both themes. `scripts/shoot.mjs` exits non-zero if there is.
- Touch targets are at least 44px on coarse pointers.
- Safe areas are respected on notched phones. Layout survives 200 percent text size.
- Content stays in a readable column on wide screens, and display type and product imagery scale up.

## Accessibility

- Text contrast is at least 4.5:1, control borders and focus rings at least 3:1, in light and dark.
- Meaning never relies on color alone. Callouts, status and chart series carry text labels.
- Every page has a skip link, one h1, no skipped heading levels and a full keyboard path.
- Components with behavior (tabs, filters, copy) follow the WAI-ARIA pattern and announce their result.

## Performance

Budgets per prerendered route, enforced by `node scripts/size.mjs` (a breach exits non-zero and fails the build). Sizes are gzip, except fonts, which are WOFF2.

- JavaScript at most 95 KB gzip for the first load: React and react-dom (about 71 KB, measured), router, hydration, theme and header. The demo island (40 KB) and code highlighting (20 KB) load lazily and are not part of it.
- CSS at most 14 KB. `styles.css` scans the common components. Docs, app, static and terminal components have opt-in entries: `styles-docs.css`, `styles-app.css`, `styles-static.css` and `styles-terminal.css`. Import each required entry after `styles.css`. Measure the combined stylesheets for each route.
- The two preloaded text fonts (Manrope and IBM Plex Mono, Latin subsets) at most 70 KB together, with `font-display: swap` and metric-matched fallbacks so the swap moves nothing.
- HTML at most 30 KB per route.
- On a throttled mobile profile, Largest Contentful Paint at or under 1.2 seconds and Cumulative Layout Shift under 0.05.
- The theme script is external, synchronous and under 1 KB, so the page never flashes the wrong theme.
- Every route is prerendered to static HTML that reads correctly with scripts off.

The numbers are a starting point, tuned from measurement. A change to a budget is made in the design document, not in the script alone.

## Gates that enforce it

| Gate | Command | Fails on |
| --- | --- | --- |
| Text, stale files, types, tests | `npm run check` | Em dashes, stale generated files (tokens, art), changed brand assets, type errors, failing tests |
| Contrast | `node packages/tokens/scripts/contrast.mjs --verbose` (also in `npm run check`) | Text under 4.5:1, control border or focus under 3:1, for both themes and every band |
| Viewport matrix | `node scripts/shoot.mjs <url> [--theme dark]` | Horizontal overflow from 320px to 5120px, small touch targets |
| Budgets | `node scripts/size.mjs [dist]` | HTML, CSS, JavaScript or font size over its budget |

More gates are added with the components and the gallery: no card or panel classes, missing simulated labels, and a missing head, header or toggle on a route.
