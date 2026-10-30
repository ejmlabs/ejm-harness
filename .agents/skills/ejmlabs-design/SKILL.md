---
name: ejmlabs-design
description: Use before changing anything visible in an ejmlabs site or app built on ejm-ui (layout, color, type, light or dark theme, motion, images, icons, accessibility or interface copy) and before calling a visible change done. A repo design skill may replace this one.
---

# Interface and brand

Containerless editorial: structure comes from type, whitespace, hairline rows and full-bleed
bands. Every page works in light and dark, with the keyboard, with reduced motion and without
scripts.

## Scope and owner

- This covers every site and app on ejm-ui. A repo with its own design system (gerry) names it in
  its AGENTS.md, and its repo design skill replaces this one.
- ejm-ui owns tokens, type, spacing, branding, themes, motion, controls and composition. A product
  supplies content, route data and accessible state.
- When a pattern is missing, add it to ejm-ui (component, token or recipe) and rebuild the
  consumers. Never build a local visual system, local CSS or a one-off component.
- The contract is `THEME.md` in the installed `@ejm/ui` package. The bar is `QUALITY.md` in the
  ejm-ui repository. Read them before choosing a pattern.

## Brand

- Two fonts only: Manrope (`font-sans`) and IBM Plex Mono Medium (`font-mono`). No other family
  may appear in computed styles. An app's code editor keeps its own font.
- Labels, metadata and code use IBM Plex Mono. Nothing under 12px; body at least 16px on phones.
- Colors come only from `@ejm/tokens`, never a literal value or a Tailwind palette name. A new
  color is a new role in `theme.json`, in both themes, passing the contrast gate.
- Use semantic names (`text-fg`, `text-muted`, `border-line`, `bg-tone`, `text-link`) for
  anything that changes with theme or band. Brand names (`sage`, `forest`, `ink`, `paper`,
  `mist`) are for art and one-off marks.
- Sage leads at about 30 percent of a page, Paper and Mist about 60, Forest under 10. Forest
  never sets a large ground.
- Use the committed logo and mark files from `@ejm/tokens`. Never type the wordmark.

## Layout and depth

- No cards, panels, tiles or tonal boxes. Group with `hairline` rows, space and bands. `bg-tone`
  is a hover or selected row fill, never a box. Tags are plain mono text, not filled chips.
- A band is a full-bleed section with its own ground (`data-band` plus `band`). No two
  neighbouring sections use the same layout.
- Only product windows, menus and overlays get a shadow (`shadow-rest`, `shadow-raised`,
  `shadow-float`). Docked panes stay flat.
- Radius only on controls (`rounded-control`, `rounded-pill`) and windows (`rounded-window`).
- One display line per page at most. Body lines stay within `max-w-measure` (68ch).
- Product visuals show real commands and screens, labelled "Simulated" unless live. No stock
  photos, gradient blobs or glass blur.

## Light and dark

- Every surface works in both themes. Never ship a change you have seen in only one.
- `theme-boot.js` is external and synchronous, before any CSS, so the first frame has the right
  theme. Without a saved choice the system preference decides.
- The theme control sits last in the header tools. `?theme=dark` and `?theme=light` are test
  overrides and are never saved.
- Use `dark:` only for what a token cannot say. Semantic colors already flip.

## Motion

- Animate transform and opacity only. Interactions finish under 300ms, a page entrance under 1.2s.
- Use the named duration and easing tokens. Never a raw time or curve.
- Under reduced motion, and with scripts off, every element is visible in its final place.
- Nothing moves without a cause: arrival, a state change or a user action.
- Text that can be the Largest Contentful Paint never starts at opacity 0. Use `animate-slide`.

## Accessibility

- Text contrast at least 4.5:1; control borders and focus rings at least 3:1, in both themes and
  on every band.
- Color is never the only signal. Status, callouts and chart series carry a word or a glyph.
- Every page has the skip link first, `main#main`, one `h1`, no skipped heading levels and a full
  keyboard path. Focus is always visible.
- Every interactive element has rest, hover, focus, pressed and disabled states. Touch targets
  are at least 44px on coarse pointers (`touch-target`).
- Behavior components follow the WAI-ARIA pattern and announce their result.
- Lists and tables have empty, loading and error states: one sentence and, where one exists, one
  action.
- Decorative art is `alt=""` or `aria-hidden`. Meaningful images say what they show.
- Links to another site use `rel="external"` and show the arrow.

## Copy

- Use the fewest words that keep the meaning. Prefer a clear visual or nothing over a sentence.
- Keep the words needed for access, consent, trust, errors and recovery.
- Familiar words, active verbs, sentence case. Name actions by their result: Save, Connect.
- State what works now. Label examples, estimates, limits and unreleased features as such. Show
  unknown as unknown, never as zero or a pass.
- No em dashes, slogans, invented adoption or customer claims. No word-count gates.

## Terminal interfaces

- Build from `ejm-tui` blocks. No boxes, frames or painted backgrounds; group with space and one
  hairline rule. Every state has a word or glyph and works under `NO_COLOR`.

## Done means seen

A visible change is done only after screenshots in light and dark at phone and desktop widths,
plus keyboard, reduced motion and console checks. Report what you checked; the rest is unverified.

## Read next

- [references/tokens.md](references/tokens.md): picking a color, band, type, space or motion token.
- [references/review.md](references/review.md): taking screenshots or reviewing before done.
