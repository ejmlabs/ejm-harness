# Native HTML applications

Use `@ejm/ui/static.css` when an application already owns native HTML, forms
and DOM state. The stylesheet is the shared design owner. It includes the same
tokens, system themes, local fonts, control recipes, page gutters, heading scale
and motion as the React components. It adds no browser JavaScript or framework.

## Install and serve

Keep the declared local UI packages available at build time. Resolve public
exports with the normal Node resolver and copy these files into your asset build:

- `@ejm/ui/static.css` as your same-origin stylesheet.
- `@ejm/ui/fonts/manrope-latin.woff2` and
  `@ejm/ui/fonts/ibm-plex-mono-500-latin.woff2` into `fonts/` beside it.
- `@ejm/ui/fonts/Manrope-OFL.txt` and `@ejm/ui/fonts/IBMPlexMono-OFL.txt`
  beside the fonts, preserving the original copyright and OFL terms.
- Project artwork from `@ejm/tokens/assets/projects/*`.

```html
<meta name="color-scheme" content="light dark">
<link rel="stylesheet" href="/shared/ui.css">
```

The static profile follows the system theme without a theme boot script,
analytics, cookies or browser storage. An application may apply a temporary
`data-theme="light|dark"` root override, but it must own and disclose that choice.
Privacy-sensitive consumers can keep the system setting as their only choice.

Regenerate shared CSS with `npm run build:static -w @ejm/ui` in ejm-ui after a
shared recipe or token change. `npm run check -w @ejm/ui` rejects a stale artifact
and enforces the existing 14 KiB gzip CSS and 70 KiB combined font limits.
Those are transfer limits, not latency measurements.

## Compose the shell

Use `header` with one `header-inner`. Its height is the shared
`--ejm-header-h`, including the safe-area treatment. Put a `brand-artwork` span
inside the `brand` home link, with a decorative image and project name. Preserve
the artwork's intrinsic dimensions; the shared profile owns its rendered size.
Do not put a second name treatment or local sizing around it.

```html
<a href="#content" data-ejm="skip">Skip to content</a>
<header data-ejm="header">
  <div data-ejm="header-inner">
    <a href="/" data-ejm="brand" aria-label="Project home">
      <span data-ejm="brand-artwork">
        <img src="/shared/project-mark.svg" width="256" height="256" alt="">
        <span>project</span>
      </span>
    </a>
    <nav data-ejm="nav" aria-label="Main">
      <ul data-ejm="nav-list"><li><a href="/account">Account</a></li></ul>
      <details data-ejm="menu">
        <summary>Menu</summary>
        <ul data-ejm="menu-list"><li><a href="/account">Account</a></li></ul>
      </details>
    </nav>
  </div>
</header>
<main id="content" tabindex="-1" data-ejm="page" data-motion="page">
  <header data-ejm="heading">
    <p data-ejm="eyebrow">Project</p><h1>Account</h1>
  </header>
  <div data-ejm="stack"><p>Page content.</p></div>
</main>
```

The desktop list and mobile disclosure contain the same links. Only one list
appears in the accessibility tree at a time. Use `aria-current="page"` on the
current link in both lists. The native summary opens and closes with Enter or
Space, including without scripts. The shared menu overlays the page instead of
moving its contents. It has bounded scrolling on a short screen.

`heading` uses the same application `text-title`, metadata and body-sized copy
as `SectionHead purpose="application"`. The page uses the shared application
rhythm. Use one h1 and choose later heading levels for the document structure.

## Native forms and feedback

Use the fewest words that preserve meaning. Let clear layout and native state
replace redundant explanation. Keep labels, trust and permission limits,
errors, recovery and accessible names explicit.

| Recipe | Structure and behavior |
| --- | --- |
| `stack` | Full-width content with the same 2rem gap as React `AppFlow`. |
| `group` | Related title, description, metadata and actions with the same 1rem gap as React `AppGroup`. |
| `section` | A hairline, heading and body with shared section spacing. |
| `form-field` | One label, control and optional help or error text, with the shared field gap. |
| `field` | Native input, select or textarea. Keep labels, required, disabled, validation and descriptions. |
| `checkbox` or `check` | Label wrapping a native checkbox and a text span. |
| `button` | Native button or an action link. Use an explicit button type. |
| `actions`, `action-row` | Controls that wrap within the available width. |
| `list`, `row` | Lists and items separated by hairlines. |
| `notice`, `status`, `empty` | Unboxed feedback and empty states. Empty notice and status nodes take no space. |
| `eyebrow`, `lede`, `muted`, `helper`, `caption` | Metadata, introduction and supporting text. |
| `metadata`, `fingerprint` | Definition lists and identifiers that wrap without widening the document. |
| `footer` | Shared footer rhythm with product information and navigation. |

Buttons use the React-free canonical `@ejm/ui/control-recipes`, also used by
React Button and Input. Native button attributes are
`data-variant="primary|secondary|ghost|link|danger"`,
`data-size="sm|md|lg|icon"` and `data-density="comfortable|compact"`.
Defaults are primary, md and comfortable. Feedback may use
`data-tone="muted|success|warning|danger"`. Pair color with words, labels and
native state. An `aria-disabled` link also needs an activation guard.

```html
<form method="post" action="/profile" data-ejm="stack">
  <div data-ejm="form-field">
    <label for="role">Role</label>
    <input id="role" name="role" data-ejm="field" required
      aria-describedby="role-help">
    <p id="role-help" data-ejm="helper">Use your current role.</p>
  </div>
  <div data-ejm="actions"><button type="submit" data-ejm="button">Save</button></div>
</form>
```

Every native button needs `data-ejm="button"` and an explicit `type`. Every
input, select and textarea needs `data-ejm="field"`, except `type="hidden"`
inputs and checkboxes wrapped in a `checkbox` or `check` label. Controls inside
hidden regions still need their recipe before the region becomes visible.

Keep ordinary form methods, actions, hidden fields and server validation in the
consumer. Styling does not require script, disable validation or change a form's
submission contract. Native fieldset, legend, details, summary and pre retain
their semantics. The hidden attribute always hides its element.

## Related content

Use `group` for facts that belong to one item. Nest it in a `list` or `row` when
items need hairlines. Use `stack` between independent sections; reserve
`form-field` for a label and its control.

```html
<ul data-ejm="list">
  <li><article data-ejm="group" aria-labelledby="package-title">
    <h2 id="package-title">harbor-search</h2>
    <p>Search local project notes.</p>
    <p data-ejm="muted">Version 1.2.0</p>
    <pre>ejm packages install harbor-search</pre>
  </article></li>
</ul>
```

The group keeps one flexible column, a minimum width of zero and wrapping for
long identifiers. The shared owner fixes its spacing; consumers choose content
and semantic elements.

## Working areas and motion

Use `setup` for a connection form and any host-verification or approval step.
It owns page gutters, a maximum outer width of 42rem and ordinary document
flow. Its h1 is fixed at 2rem with weight 600. Compose the content with
`heading`, `stack`, `form-field`, `field`, `actions`, `notice` and native forms.
Keep the trust disclosure and required acknowledgment visible before submission.
The other native page headings keep their existing scale.

After authenticated session data arrives, switch the main region from `setup`
to `workspace`. Change its connection section from `stack` to `workspace-header`,
then show the `session`. Keep one copy of each identifier and control. The
consumer owns this state and its timing; the stylesheet does not perform
pairing or infer permission from an open socket.

`workspace` is one full-width column. `workspace-header` keeps the title, status
and connection actions compact; its h1 uses the same fixed 2rem/600 heading.
Put connection metadata and fingerprints in a native `details` disclosure with
a labelled `summary`, using `metadata` and `fingerprint` inside it. Metadata
pairs use two columns from 40rem and one column below it. The disclosure needs
no script and does not reserve a sidebar or place a box around the content.

The `session` follows the header without a side border or a second scroll area.
Use `session-header`, `transcript`, `message`, `message-label`, `composer` and
`compose-title` for its contents. The transcript wraps long text and scrolls
within a maximum of 62svh. At widths of at least 64rem and heights of at least
48rem, the workspace fits below the shared header and lets the transcript use
its remaining height, with an 8rem minimum. The workspace does not clip its
contents: expanding details or adding feedback can still make the document
scroll. On phones and shorter windows, the whole page stays in natural flow
and the transcript keeps its own bound. Controls remain outside that scroll
area. Shared presentation does not bound retained data; the consumer must
still enforce its output and entry limits.

The initial document must hide an unused session with the native `hidden`
attribute, leaving setup painted and keyboard reachable. After a deliberate
stage change, focus one visible heading or field. Do not move focus on streamed
output or repeat it during reconnect. Keep setup errors outside hidden regions;
associate field errors with their inputs and keep recovery controls reachable.
A later transport loss can retain the session and pending-action feedback.
Explicit disconnect-and-clear restores setup and a visible focus target. These
are consumer responsibilities, verified with focused browser tests.

`data-motion="page"` on a page or sidebar uses the same transform-only entrance
as the React application Page. Its content remains painted from first paint. Use `data-motion="enter"` for a discrete state becoming visible and
`data-motion="arrive"` for a newly inserted block. They move 6px and fade over
200ms or 320ms. Update streaming text in the same node so it does not replay an
entrance for every token. Never wait for motion before revoking access, disabling
a control or clearing sensitive state. Reduced motion shows final content with
no travel or animation. No content is hidden while it waits for a script.

## Enforce ownership in the build

Add build-only `@ejm/site` and call its public native check before compiling or
copying assets:

```js
import { checkNativeDesign } from '@ejm/site/native';

checkNativeDesign({
  root: process.cwd(),
  source: ['src', 'web'],
  stylesheetHref: '/shared/ui.css',
  documents: [{ name: '/account (signed in)', html: renderAccount(account) }],
});
```

Include every authored template and controller source directory, then supply
representative finished route documents for conditional page branches. A build
must not skip its ownership check in source-only or fixture mode. Serve the exact
public stylesheet bytes at the configured same-origin URL. Compatibility CSS
may contain only an import of that URL or `@ejm/ui/static.css`.

The check uses maintained Vite, PostCSS and parse5 parsers in Node. It rejects
consumer classes, inline styles, local CSS rules, style elements, extra
stylesheets, presentational HTML attributes, local SVG or canvas drawings, unknown shared recipe or state values, unmarked controls, buttons without an
explicit type, and DOM class, style or raw markup setters. Finished HTML is checked after interpolation, so a branch cannot
hide styling from the template check. Errors name the source or generated route
with a line and column. Generated build and test output directories are excluded
from the authored source scan.

This check enforces code ownership. It is not a security sandbox or a proof of
all runtime states. Review dynamic state values and use focused browser checks
for semantics, loaded assets, themes, keyboard access, bounded scrolling, phone
layout, reduced motion and no-script forms. Product transport, auth, escaping and
mutation checks remain in their owning consumer.
