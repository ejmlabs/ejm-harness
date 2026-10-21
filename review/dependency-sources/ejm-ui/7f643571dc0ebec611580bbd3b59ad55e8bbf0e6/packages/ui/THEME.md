# Theme contract

The public theme contract for `@ejm/ui/styles.css` and its opt-in entries. If a name is not in this file, it does not exist. Colors never appear as literals in components: they come from the semantic names below, which come from `packages/tokens/theme.json`.

## Use it

```css
/* a site's entry stylesheet */
@import "@ejm/ui/styles-site.css" source(none);
```

```html
<head>
  <meta name="color-scheme" content="light dark" />
  <script src="/theme-boot.js"></script>   <!-- synchronous, before any CSS paints; file: @ejm/ui/theme-boot.js -->
  <!-- preload the two fonts: render the props in `fontPreloads` from @ejm/ui as <link> tags -->
</head>
```

Serve `packages/ui/public/theme-boot.js` as `/theme-boot.js` (an external file, so `script-src 'self'` holds). Each profile scans its own shared components. `styles.css` scans `components`, `layout`, `shell`, `hooks` and `motion`. Docs, app, static and terminal components use opt-in entries:

```css
@import "@ejm/ui/styles.css";
@import "@ejm/ui/styles-docs.css";   /* only if the site renders DocsLayout, DocsSidebar, CodeBlock, ParamTable, Callout, Pager, Contents */
@import "@ejm/ui/styles-app.css";    /* only if it renders Dashboard, ChartLine, ChartBar, Legend, ActivityList */
@import "@ejm/ui/styles-static.css"; /* only if it renders NativeCheckbox, Disclosure, ActionRows, DocumentContent */
@import "@ejm/ui/styles-terminal.css"; /* only if it renders TerminalWorkspace and the Terminal* views */
```

Import the partial for every folder whose components you render, or their classes are missing. `node packages/ui/scripts/check-sources.mjs` checks that each folder gets its classes from the right entry and that a default folder never imports from docs or app. Applications may scan their own source folders. Sites use the stricter shared design guard. Shared package source coverage belongs to the package, through its public stylesheet exports; do not scan or exclude private package files.

`styles-site.css` is the smaller public profile for marketing and project sites. It includes shared site layout, branding, shell, Button, hooks and motion. Add the same docs or app partials when needed. `styles.css` remains the full default profile for component galleries and apps. Both import one internal base with identical tokens, fonts, type, motion and global rules. `base.css` is private. Product demo source coverage comes from the public `@ejm/demos/*.css` profiles.

Use `source(none)` on the entry import. This retains the profile's explicit
component sources and avoids scanning unrelated installed package files. Sites
compose public components and do not add utility classes or source directives.
The starter and installed-package proof use this setup.

## Native HTML profile

Keep visible copy brief. Use no text where the layout or native state is clear; retain labels, trust disclosures, permission limits, errors, recovery and accessible names.

`@ejm/ui/static.css` is the compiled, React-free profile for applications that already own native HTML and state. It shares the current Header and Brand geometry, application title and form rhythm, tokens, system themes, local fonts, canonical controls and calm motion. Consumers supply semantic content and events through the fixed `data-ejm` recipes, with no classes, inline styles or local CSS rules.

See [Native HTML applications](../site/native-html.md) for the shell and form markup, fixed setup and one-column workspace recipes, compact connection details, asset copying, no-script forms, bounded output, motion and reduced-motion behavior. Setup and workspace-header h1 headings use a fixed 2rem size and weight 600; other native pages and React headings retain their current scale. It also documents the public `@ejm/site/native` build check for authored templates, DOM presentation hooks and finished route documents.

Run `npm run build:static -w @ejm/ui` after shared recipe, token or font changes. The shared check rejects stale output and keeps the 14 KiB gzip CSS and 70 KiB combined font limits. This profile does not load theme-boot.js, analytics or a browser runtime and does not store a theme choice. Native privacy-sensitive pages can continue to use the system palette and ordinary same-origin forms.
The terminal module lives in `src/terminal` and keeps the public import
`@ejm/ui/components/TerminalWorkspace`. Its structural utilities belong to
`styles-terminal.css`; products should compose the public components rather
than copy their markup or utility classes. `NativeCheckbox`, `Disclosure`,
`ActionRows` and `DocumentContent` live in `src/static` and use
`styles-static.css`. All keep root exports. `Disclosure`, `ActionRows` and
`DocumentContent` keep their shell or layout deep aliases. `NativeCheckbox`
uses `@ejm/ui/components/NativeCheckbox`. A page that
uses both groups needs both partials. All entries share the same tokens, fonts
and theme.

The Tailwind default palette, radius, shadow and font namespaces are reset. `bg-red-500`, `rounded-xl`, `shadow-md`, `font-serif` and `drop-shadow-*` do not exist, so a color or shape outside the system fails to compile.

## Themes

| Mechanism | Behavior |
| --- | --- |
| `<html data-theme="light\|dark">` | Sets the page theme. Light is the default. |
| No `data-theme` on the root | The system `prefers-color-scheme` decides, with no script. |
| `data-theme` on any element | Starts a new theme for that subtree (for a preview of both themes side by side). The nearest one wins. |
| `?theme=dark\|light` in the URL | Test override. `theme-boot.js` applies it and never saves it. |
| `dark:` variant | `dark:` follows the same rules. The semantic colors already flip, so use `dark:` only for what a color token cannot say. |

`theme-boot.js` (414 bytes, no dependencies) runs before first paint and always sets `data-theme` on `<html>`. Order: `?theme=`, then the saved choice, then the system preference. The saved choice is the `ejm-theme` cookie on `ejmlabs.com` and its subdomains (exact host `ejmlabs.com` or a name ending in `.ejmlabs.com`), and `localStorage["ejm-theme"]` on any other host. It only reads. The theme hook (task 5.1) writes, and must use exactly the attributes in `THEME_COOKIE` from `@ejm/tokens`:

```
ejm-theme=dark; Domain=.ejmlabs.com; Path=/; Max-Age=31536000; SameSite=Lax[; Secure on https]
```

Because the boot script always sets the attribute, the hook must also follow the system preference while the visitor has not chosen (listen to `matchMedia("(prefers-color-scheme: dark)")` and update `data-theme`). Without the script, the CSS alone follows the system.

## Bands

A band is a full-bleed section with its own ground. Put `data-band="paper|mist|sage|ink"` on the element, plus the `band` utility to paint it. Every band sets the same variables for its whole subtree, so children only ever say `text-fg`, `text-muted`, `border-line` and the rest, and they are right on every ground.

```html
<section data-band="sage" class="band py-rhythm">
  <div class="page"> ... </div>
</section>
```

Bands may nest (the nearest wins), and `[data-band]` works on the same element as `[data-theme]`. The page itself is the Paper band.

| Band | Light theme | Dark theme | Use |
| --- | --- | --- | --- |
| `paper` | ground `F6F5EF`, text Ink | ground `131614`, text `E3E5DC` | The page. About half of a page with Mist. |
| `mist` | ground `E8EDE3`, text Ink | ground `1C211D`, text `E3E5DC` | The quiet alternate. |
| `sage` | ground `8A9A7B`, text Ink | the same: Sage stays Sage | The lead color, about 30 percent of a page. |
| `ink` | ground `202721`, text Paper | the same: Ink stays Ink | A dark moment in either theme. |

Paper and Mist together make about 60 percent of a page, Sage about 30, and Forest under 10. Forest never sets a large ground, which is why it is not a band.

Sage rules, forced by the numbers (Ink on Sage is 5.09:1, Forest on Sage is 3.52:1):

- All text on Sage is Ink. `text-muted` and `text-link` are also Ink there. Make hierarchy with size, weight and the mono face, not with a lighter color.
- Forest on Sage is for large type only: use `text-link-large` (display type, 24px and up).
- On Sage, `primary` is Ink (the button is an Ink pill with Paper text), and status colors are Ink. Status on Sage must carry a word or glyph, never color alone. The six hues are not defined for Sage, so charts do not sit on Sage.

### What a band sets

CSS variable, Tailwind color name and meaning. All are set by every band.

| Variable | Tailwind name | Meaning | Paper light | Sage | Ink |
| --- | --- | --- | --- | --- | --- |
| `--ejm-ground` | `bg-ground` | The band background | Paper | Sage | Ink |
| `--ejm-fg` | `text-fg` | Body and heading text | Ink | Ink | Paper |
| `--ejm-muted` | `text-muted` | Secondary text, 4.5:1 | Ink 72% | Ink | `A8B4A2` |
| `--ejm-line` | `border-line`, `divide-line` | Hairline rules. Visible, not a control edge (1.4:1 minimum) | Ink 22% | Ink 32% | `465349` |
| `--ejm-tone` | `bg-tone` | Hover and selected row fill, a step off the ground. Not a box | Mist | Paper 18% | `262C27` |
| `--ejm-field` | `bg-field` | Input background | White | Paper | `131614` |
| `--ejm-link` | `text-link` | Link and accent text, 4.5:1 | Forest | Ink | `B8C6AA` |
| `--ejm-link-large` | `text-link-large` | Accent for large type only, 3:1 | Forest | Forest | `B8C6AA` |
| `--ejm-control` | `border-control` | Control borders (inputs, outline buttons), 3:1 | `7D877F` | Ink | `6A7C6C` |
| `--ejm-focus` | `ring-focus`, `outline-focus` | Focus ring, 3:1 | Forest | Ink | Sage |
| `--ejm-primary` | `bg-primary` | Primary button fill | Sage | Ink | Sage |
| `--ejm-on-primary` | `text-on-primary` | Text on `primary`, 4.5:1 | Ink | Paper | Ink |
| `--ejm-primary-hover` | `bg-primary-hover` | Primary hover fill | Forest | Forest | Sage 70% |
| `--ejm-on-primary-hover` | `text-on-primary-hover` | Text on the hover fill | Paper | Paper | Ink |
| `--ejm-code` | `bg-code` | Code block ground | Forest | Ink | `131614` |
| `--ejm-on-code` | `text-on-code` | Code block text | Paper | Paper | `E3E5DC` |
| `--ejm-success` `--ejm-warn` `--ejm-danger` `--ejm-info` | `text-success` `text-warn` `text-danger` `text-info` | Status text, 4.5:1 on ground and tone. Also usable as `bg-*` fills | greens, ambers, reds, blues | all Ink | light-on-dark set |
| `--ejm-on-danger` | `text-on-danger` | Text on a `bg-danger` fill, 4.5:1 | Paper | Paper | Ink |
| `--ejm-hue-1` to `--ejm-hue-6` | `hue-1` to `hue-6` | The six chart hue roles, 3:1 on the ground. Not defined for Sage | blue, violet, green, amber, rose, cyan | n/a | light-on-dark set |
| `--ejm-scheme` | none | `light` or `dark`; sets `color-scheme` so form controls and scrollbars match | light | light | dark |

Full values for every band and theme are in `packages/tokens/generated/tailwind.css`. Every pair above is gated by `node packages/tokens/scripts/contrast.mjs --verbose` (254 pairs; run as part of `npm run check`). The only pairs below their usual level, by design: `primary` against the ground is 2.75:1 on Paper and 2.53:1 on Mist, so a Sage button is identified by its label (5.09:1), not its edge; use `border-control` for any control that needs a visible boundary.

### Variants and utilities for bands

- `band-paper:`, `band-mist:`, `band-sage:`, `band-ink:` apply inside that band (`band-sage:text-link-large`).
- `band` paints `background: var(--ejm-ground); color: var(--ejm-fg)`.
- `scheme-page` makes an object (a product window, a code well) keep the page theme even inside a Sage or Ink band.

## Colors, all of them

- Brand, fixed, always emitted as variables: `sage` `forest` `ink` `paper` `mist` `white` (`bg-sage`, `text-forest`, `border-ink`, and opacity such as `bg-sage/40`). Use brand colors for art and one-off marks; use the semantic names for anything that changes with theme or band.
- Semantic: the table above.
- shadcn names, so copied-in shadcn source works: `background` (ground), `foreground` (fg), `muted-foreground` (muted), `popover` (ground), `popover-foreground` (fg), `primary-foreground` (on-primary), `secondary` and `accent` (tone), `secondary-foreground` and `accent-foreground` (fg), `destructive` (danger), `destructive-foreground` (on-danger), `border` (line), `input` (control), `ring` (focus). `primary` is shared. **Not aliased on purpose:** `card`, `card-foreground` (there are no cards), and `bg-muted` (our `muted` is a text color: use `bg-tone`).
- Product windows and editors on the web, from the ui theme, following the page theme and never the band: `ui-ground` `ui-panel` `ui-editor` `ui-raised` `ui-overlay` `ui-hover` `ui-line` `ui-line-strong` `ui-text` `ui-text-strong` `ui-text-muted` `ui-text-subtle` `ui-primary` `ui-on-primary` `ui-focus` `ui-accent` `ui-accent-strong` `ui-tint-1` to `ui-tint-3` `ui-error` `ui-warning` `ui-success` `ui-special` `ui-info` `ui-control-border` `ui-diff-add` `ui-diff-del` `ui-diff-add-bg` `ui-diff-del-bg` `ui-selection` `ui-dim`, `ui-hue-1` to `ui-hue-6`, and syntax `syn-comment` `syn-keyword` `syn-string` `syn-number` `syn-type` `syn-type-parameter` `syn-function` `syn-macro` `syn-parameter` `syn-property` `syn-constant` `syn-namespace` `syn-identifier` `syn-delimiter`. Plain variables are `--ejm-ui-*` and `--ejm-ui-syn-*`.
- Native product styles can load the complete `--ejm-ui-n0` to `--ejm-ui-n15` scale through `@ejm/tokens/neutral.css`. It follows the page theme. The public `@ejm/demos/ide-preview.css` profile includes it.

Use `text-*` and `bg-*` freely; there is no card color. Tags are plain mono text, not filled chips.

## Type

Fonts: Manrope (variable, weights 200 to 800) as `font-sans`, IBM Plex Mono Medium (500 only) as `font-mono`. Nothing else loads. `font-synthesis: none`, `font-display: swap`, and metric-matched fallbacks (`Manrope Fallback`, `IBM Plex Mono Fallback`, with the `size-adjust` values from the old kit) so the swap moves nothing.

| Utility | Range at the base scale (320 to 1919px) | Line height | Tracking | Weight |
| --- | --- | --- | --- | --- |
| `text-display-lg` | 44px to 140px (reaches 140 at 1370px) | 0.96 | -0.045em | 600 |
| `text-display` | 36px to 96px | 1.0 | -0.04em | 600 |
| `text-title` | 28px to 56px | 1.06 | -0.035em | 600 |
| `text-heading` | 20px to 32px | 1.2 | -0.02em | 600 |
| `text-lead` | 19px to 26px | 1.5 | 0 | 400 |
| `text-stat` | 52px to 136px | 0.95 | -0.04em | 600 |
| `text-body` | 16px to 20px (18.4px at 1440px, 20px from 2000px) | 1.6 | 0 | 400 |
| `text-meta` | 13px to 14.4px, IBM Plex Mono | 1.5 | +0.02em | 500 |

From 1920px the five largest sizes (`display-lg`, `display`, `title`, `heading`, `stat`) multiply their maximum by `--ejm-scale` (1.125 at 1920, 1.25 at 2560, 1.375 at 3200, 1.5 from 4400). So `display-lg` tops out at 157px at 1920, 175px at 2560, 192px at 3200, 210px at 4400 and above. Body, lead and meta never scale past their caps. The root font size is never scaled, so `rem` and the Tailwind spacing scale mean the same thing on every screen.

Also: Tailwind's own `text-xs` to `text-9xl` still exist for dense app UI (nothing under 12px). `h1` to `h6` get `text-wrap: balance`, and `p`, `li`, `dd`, `blockquote` get `text-wrap: pretty`. `tabular-nums` (a built-in utility) gives tabular figures in Manrope and is what stats, versions and table cells use. Reading width: `max-w-measure` (68ch).

Font files: `packages/ui/src/fonts/manrope-latin.woff2` (22,856 bytes) and `ibm-plex-mono-500-latin.woff2` (11,680 bytes), 34,536 bytes together, down from 93,968 for the full WOFF2s (the TTFs stay in `@ejm/tokens` as the source). Latin, Latin-1, general punctuation, currency and a few symbols; the mono also has arrows, box drawing, block elements and the check mark. Both `@font-face` rules carry the same `unicode-range`. Rebuild with `node packages/ui/scripts/build-fonts.mjs` (runs `pyftsubset`, or `uv run --with fonttools --with brotli pyftsubset` when it is not installed). Preload both with `fontPreloads` from `@ejm/ui`.

## Page column, gutters and rhythm

`page` is the content column. Bands stay full bleed; the column inside them steps up with the screen. `--ejm-page-max` is the column's outer width including its gutters.

| From width | Gutter | `--ejm-page-max` | `--ejm-scale` |
| --- | --- | --- | --- |
| 320px | 16px | 1200px | 1 |
| 420px | 24px | 1200px | 1 |
| 760px | 48px | 1200px | 1 |
| 1600px | 64px | 1360px | 1 |
| 1920px | 72px | 1520px | 1.125 |
| 2560px | 80px | 1840px | 1.25 |
| 3200px | 88px | 2240px | 1.375 |
| 4400px | 96px | 2800px | 1.5 |

The gutter is never smaller than `env(safe-area-inset-left/right)`. Pages need `<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">` for that to work.

| Name | Use |
| --- | --- |
| `page` | `max-inline-size: var(--ejm-page-max); margin-inline: auto; padding-inline: gutter`, width 100%. |
| `page-inset` | Padding only, for a full-bleed element (a sticky header) whose content must line up with the `page` column. |
| `py-rhythm`, `pt-rhythm`, `gap-rhythm`, ... | The vertical rhythm of a band, 72px to 160px, times `--ejm-scale`. `*-rhythm-sm` is 40 percent of it. |
| `gap-field`, `gap-x-field`, `gap-y-field` | Label, control and help groups: `calc(var(--spacing) * 2)`, normally 8px. Derived from the existing Tailwind spacing scale. |
| `gap-content`, `gap-x-content`, `gap-y-content` | Application content and form groups: `calc(var(--spacing) * 6)`, normally 24px. Derived from the same scale. |
| `px-gutter`, `p-gutter`, ... | The current gutter. |
| `max-w-page`, `max-w-measure` | Column width and 68ch reading width. |
| Breakpoints | Tailwind defaults, plus `xs` 420px, `3xl` 1920px, `4xl` 2560px, `5xl` 3200px, `6xl` 4400px (the same steps as the table). |

`--ejm-header-h` (default 4.5rem) feeds the page `scroll-padding-top`, so anchors and focused headings land below a sticky header. A sticky header component sets it.

`Page variant="application"` combines the column with the shared application rhythm. `SectionHead purpose="application"` stacks a title heading and body-sized copy. Page openings use `purpose="page"`; ordinary sections use the default `purpose="section"`. Sites cannot pick a different heading size or alter the recipe with classes.

Public sites use `Hero`, `SectionHead`, structured shell identity and the recipes in `@ejm/ui/layout/SiteContent`. `@ejm/site/vite` checks that site source contains no local styling. Change typography, spacing, branding or motion in ejm-ui, then rebuild the sites. Product data, words, routes and accessible state remain in the site. See the layout README for the public composition API. Lower-level utility recipes in this document describe shared implementation, not consumer escape hatches.

## Elevation and radius

- `shadow-rest`, `shadow-raised`, `shadow-float`: soft layered shadows, deeper in dark with a faint top edge of light. **Reserved for product windows, menus and overlays.** Nothing else has a shadow. Variables: `--ejm-shadow-rest|raised|float`.
- `rounded-control` (6px) for buttons, inputs, tags, toggles; `rounded-pill` (999px) for pills and the theme toggle; `rounded-full` and `rounded-none` as usual. `rounded-compact` (5px) is the control radius of the compact density that applications use (see COMPONENTS.md). `rounded-window` (16px) for product windows, menus and overlays only. `rounded-sm|md|lg` equal `rounded-control` so shadcn source works; prefer `rounded-control` in new code. There is no card or panel radius.

## Motion

Transform and opacity only (draw-on uses stroke offset, as specified). Every animation utility is wrapped in `prefers-reduced-motion: no-preference`, so under reduced motion nothing moves and every element is in its final state. Durations also collapse to 1ms under reduced motion.

The shared theme control uses the native View Transitions API for a 320ms crossfade after an explicit click or key press. The preference still saves if a transition is skipped or cannot start; browsers without the API and visitors who prefer reduced motion switch immediately. Initial theme selection and system-theme updates do not start a transition. The shared SSG stylesheet supplies the crossfade timing. Header links and the narrow menu use the existing color and fade utilities. Keep first-paint text and product captures visible; use a transform-only entrance for large hero text.

| Token | Value |
| --- | --- |
| `duration-fast`, `duration-base`, `duration-slow` | 120ms, 200ms, 320ms (`--ejm-duration-fast|base|slow`) |
| `--ejm-duration-entrance` | 640ms, the hero and page entrance |
| `--ejm-duration-draw` | 700ms |
| `--ejm-stagger` | 60ms per step |
| `ease-standard` | `cubic-bezier(0.2, 0.7, 0.3, 1)`, state changes |
| `ease-enter` | `cubic-bezier(0, 0, 0.2, 1)`, things arriving |
| `ease-exit` | `cubic-bezier(0.4, 0, 1, 1)`, things leaving |
| `ease-emphasis` | `cubic-bezier(0.16, 1, 0.3, 1)`, the one hero entrance and draw-on |

Animation utilities, played once on mount with `animation-fill-mode: backwards`:

- `animate-fade`, `animate-reveal` (320ms, rises 16px), `animate-enter` (640ms, rises 12px, `ease-emphasis`), and `animate-slide`, the same 640ms rise with no fade. Use `animate-slide` on text that can be the Largest Contentful Paint (the `Hero` headline and lead): an element that starts at opacity 0 is not painted yet, so LCP waits for the whole fade (measured on `examples/starter`: LCP 1488 ms with the fade, 704 ms with the slide, FCP 703 ms).
- Stagger: `style={{ '--ejm-i': n }}` on the Nth child. The index stops at 8, so a staged entrance is at most 8 x 60 + 640 = 1120ms, under the 1.2s limit. `--ejm-delay` adds a fixed wait. Reveal offsets can be changed per element with `--ejm-reveal-x`, `--ejm-reveal-y` and `--ejm-reveal-scale`.
- `draw-on` on an inline `<svg class="art">`: each `<path pathLength="1">` draws on once, in order (700ms plus 40ms per path, at most 11 steps). Without the class, or under reduced motion, the art is fully drawn. Add the class from script (in view), so without scripts the final state shows at once.

**Scroll reveal contract for the hooks (tasks 5.1 and 5.2).** The `useReveal` hook sets `data-reveal` and nothing else is hidden by CSS:

| Attribute | Meaning |
| --- | --- |
| `data-reveal="pending"` | Hidden (opacity 0, offset). Set by the hook only on elements below the fold, before first paint, while it is running. |
| `data-reveal="in"` | Plays `ejm-reveal` once and ends in the natural state. Set when the element enters the viewport; the hook then stops observing. |
| `data-reveal-from="up\|fade\|scale\|left"` | Direction. Default is up. |
| `--ejm-i`, `--ejm-delay` | Stagger and delay, as above. |

Under reduced motion, with scripts off, and in print, `pending` hides nothing. The hook must also set `in` on a timer if the observer never fires, so content can never stay hidden.

## Focus, touch and rules

- `:focus-visible` gets a 2px `--ejm-focus` outline with 2px offset, globally. `focus-ring` does the same for a custom control that removes it. Shadcn's `ring-*` classes work with `ring-focus` / `ring-ring`.
- `touch-target`: at least 44 by 44px on coarse pointers. Use it on block, flex or inline-flex controls; inline links in running text are exempt.
- `compact-target`: at least 32 by 32px on fine pointers, for the compact density. Use it with `touch-target` (the two media queries never apply together).
- `sr-inline`: screen reader text that stays in the flow (inside links and table cells), where `sr-only` could sit outside a scroll region.
- `animate-slide`: the `animate-enter` rise without the fade, for text that can be the LCP.
- `hairline` (1px line on the bottom edge), `hairline-t` (top edge). `border` utilities default to the line color.
- `link` (color, underline, 2px on hover) for classed anchors. Bare `<a>` elements with no class already get it.
- `skip-link`: first focusable element on every page, `<a href="#main" class="skip-link">`. Visible on focus.
- `duration-*`, `ease-*`, `shadow-*`, `rounded-*` as above. `sr-only` is Tailwind's own.

### Shared component structure

These public, implementation-owned utilities keep shared component output
small. Prefer the components when composing a product; keep their markup and
state hooks with `@ejm/ui`. The native and document utilities below belong to
`styles-static.css`. Import that entry after `styles.css` for the static parts.

- `native-check-label`, `native-check`: checkbox label target and native input
  with the shared accent color. The platform supplies the checkmark.
- `disclosure-summary`, `disclosure-content`, `disclosure-menu`: native
  disclosure structure and bounded menu presentation. The platform supplies
  the open/closed marker. Menus retain a border in forced colors.
- `document-content`: shared reading width, heading and form rhythm, wrapped
  preformatted text and status/error presentation.
- `terminal-*`: the workspace, header, navigation, transcript, status and
  composer structure in the opt-in `styles-terminal.css` entry. Import that
  entry after `styles.css` when rendering the `Terminal*` components.

## Base styles

`html`: ground background, no font synthesis, text-size-adjust 100%, smooth scrolling (not under reduced motion), `scroll-padding-top` for a sticky header and the notch. `body`: ground and fg, `text-body` size, 1.6 line height, antialiased. `::selection`: fg on ground, inverted. `*`: border color is `line`. `:focus-visible` as above. Forced colors keep the system focus color.

## Symbol art

Generated art lives in `packages/ui/src/art`. The inline files (`art/inline/*.svg`) use `class="art"`: lines take `--ejm-link`, `.art-accent` paths take `--ejm-primary`, `.art-fill` paths fill with `--ejm-primary`, so they read on every band. The standalone `*-light.svg` and `*-dark.svg` files use brand hex values and are for `<img>` and backgrounds on Paper, Mist or Sage (light) and Ink or a dark page (dark). Art is decorative: `aria-hidden` or `alt=""`.

## Do not

- No cards, panels, tiles or tonal boxes. Group with `hairline` rows, space and bands.
- No shadow outside product windows, menus and overlays; no radius except on controls and windows.
- No literal colors and no Tailwind palette names. No second font.
- No animation on anything but transform and opacity (plus the draw-on stroke offset), and always with a final state that needs no script.

## Measured

Gallery tokens route (4 bands, type scale, elevation, about 60 distinct utilities): built CSS 31.2 KB, 6.99 KB gzip, including Tailwind's reset, the full band and theme variables, the window colors (about 1.4 KB gzip) and all five `@font-face` rules. The 14 KB gzip budget is measured on this near-empty page and is re-measured when the components land (`node scripts/size.mjs`).
