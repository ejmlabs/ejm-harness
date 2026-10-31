# Review a visible change

Look at the built result, not the source. Keep scripts and screenshots in a scratch folder
outside the repository. A text-only change needs only the console check; the rest of this page is
for layout, style and behavior changes.

## Screenshots

1. Build and serve the output on `127.0.0.1` (a site's `dist`, or the app's dev server).
2. Run ejm-ui's `scripts/shoot.mjs` against the URL twice, with `--theme light` and
   `--theme dark`. It shoots every size from 320px to 5120px with reduced motion, and exits 1 on
   horizontal overflow or a tap target under 44px. Add `--full` for whole pages.
3. Without that script, drive Chrome with `playwright-core` and the installed browser at 320,
   375 and 1440 for a site (1280 for an app), in both themes. Check that
   `document.documentElement.scrollWidth` equals `clientWidth` at 320 and 375.
4. Set the theme before load: `?theme=dark` on a site, or the app's stored preference (for
   example `localStorage["ejm-theme"]`) in an app.
5. Open the full-resolution images and look at them. Judge finish from pixels, not from logs.

## Walk the page

- Fonts: `getComputedStyle` reports Manrope or IBM Plex Mono on every element, and
  `document.fonts` shows both loaded. A fallback can look right in a screenshot.
- Keyboard: Tab from the top. The skip link comes first, focus is visible on every stop, Escape
  closes menus and returns focus, and no stop is a trap.
- Reduced motion: emulate it and reload. Everything sits in its final place at once.
- Scripts off: text, links and downloads still work on a prerendered page.
- States: rest, hover, focus, pressed, disabled; loading, empty and error for each list.
- Text size: the layout survives 200 percent text.
- Console: empty. A 404 for one resource is a finding.

## Check the bar

- One subject and at most one primary action per section.
- No card, panel, tile, tonal box or decorative shadow crept in.
- Contrast holds on every band in both themes. The ejm-ui gate is
  `node packages/tokens/scripts/contrast.mjs --verbose`, run from an ejm-ui checkout after a
  token change.
- Copy: every sentence earns its place, nothing repeats a label already on screen, and no em
  dashes.
- Load time: the repository's performance gate still passes.

## Report

Name what you checked: URL, themes, widths, keyboard, reduced motion and scripts off. Name what
you did not check. A skipped check is unverified, not a pass.
