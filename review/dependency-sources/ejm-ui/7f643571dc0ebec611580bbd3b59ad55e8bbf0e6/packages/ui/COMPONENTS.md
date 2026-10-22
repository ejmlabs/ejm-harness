# Components

Every component in `@ejm/ui`, the primitive under it (Radix or native) and why. `node packages/ui/scripts/check-components.mjs` fails when a component file is missing here, and when anything is named or classed card, panel or tile. The states each component renders are in `src/catalog/*.json`, merged into `src/catalog.json` by `node packages/ui/scripts/build-catalog.mjs`, which `scripts/parity.mjs` compares with `packages/core/contract.json`.

Rules every component follows: Tailwind utilities from `THEME.md` only, no literal colors, no new CSS files, `touch-target` (44px on coarse pointers) on every control, the global 2px focus ring, motion on transform and opacity only with the final state under reduced motion. Only product windows, menus and overlays have shadows. The gallery documents these APIs at `/docs/components` and shows control examples in both themes at `/components`.

How to read the primitive column: **native** means a platform element does the job, **Radix** means a Radix primitive earns its weight (behavior that is hard to get right by hand), **core** means the framework-neutral model in `@ejm/core`.

## Native HTML applications

Existing applications that own a native DOM may use `@ejm/ui/static.css`. This compiled profile has no browser JavaScript dependency. Its `data-ejm` attributes provide the shared shell, application layout, native fields and buttons, feedback, and discrete state entrances. See the native HTML profile in `THEME.md` for the complete contract, asset paths, accessibility requirements, and reduced-motion behavior.

`@ejm/ui/control-recipes` exports `buttonVariants` and `fieldClass` without importing React. The React wrappers and the static build use those same recipes. Fixed native ghost buttons in the default density can join the exported `buttonBaseClass`, `buttonGhostClass` and `buttonMediumClass` parts. This keeps the same shared style without loading the variant resolver or React slot and icon adapters; use `buttonVariants` when the choices are dynamic. This keeps native HTML adapters narrow while preserving their existing state and event ownership. The static profile is shared infrastructure; consumers compose its public contract and keep local styles out of that adapter.

## Controls and overlays (primitives workstream)

Source: `src/components/ui/`, copied-in shadcn/ui style on React 19 with `cn` and `class-variance-authority`. `components.json` records the shadcn setup (aliases, no CSS variables file: the theme is `styles.css`).

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| `Button` | `components/ui/button.tsx` | native `<button>`, Radix Slot for `asChild` | A button needs no library. Slot lets the same look wrap a router link. Variants primary (Sage, Ink text), secondary, ghost, link; sizes sm, md, lg; `arrow` slides 4px on hover. Pills stay on one line; the link variant wraps. Variant `danger` (red fill), size `icon` (square, needs an `aria-label`), `density="compact"` (see below). |
| `Input` | `components/ui/input.tsx` | native `<input>` | The platform gives labels, autofill, mobile keyboards and validation. `Input` only adds the control look. |
| `Textarea` | `components/ui/textarea.tsx` | native `<textarea>` | Same as Input. |
| `Select` | `components/ui/select.tsx` | native `<select>` | The native picker is better on phones, has full keyboard and typeahead support, and costs no JavaScript. Radix Select was rejected: it re-implements the same and adds about 10 KB gzip. |
| `Switch` | `components/ui/switch.tsx` | Radix Switch | A `role="switch"` button with form value, controlled and uncontrolled use. Native checkbox styling cannot do the thumb without extra markup. |
| `Checkbox` | `components/ui/checkbox.tsx` | Radix Checkbox | The indeterminate (mixed) state and a styled box. |
| `NativeCheckbox` | `static/NativeCheckbox.tsx` | native `<input type="checkbox">` inside `<label>` | Named form submission, required validation, label activation and reset work before hydration. Uses the platform checkmark with the shared accent color. The standard `Checkbox` keeps its Radix API. |
| `Badge` | `components/ui/badge.tsx` | native `<span>` | Plain mono text with a slash or dot mark. Not a pill, not boxed. |
| `Separator` | `components/ui/separator.tsx` | native `<div>` | A hairline. `role="none"` when decorative. |
| `Tooltip` | `components/ui/tooltip.tsx` | Radix Tooltip | Hover and focus timing, Escape, collision handling and `aria-describedby`. Hard to hand-roll well. No shadow: it is a mark, not a surface. |
| `Tabs` | `components/ui/tabs.tsx` | core (`tabsReducer`) on native buttons | The same arrows, Home, End, wrap and roving tabindex as the Ink and Rust tabs. No Radix needed, so no extra weight. |
| `Dialog` | `components/ui/dialog.tsx` | native `<dialog>` | `showModal()` traps focus, makes the page inert, handles Escape and restores focus. The component adds the label, the backdrop click and an explicit focus return. Radix Dialog was rejected as the heavier path. |
| `Menu`, `MenuTrigger`, `MenuContent`, `MenuItem`, `MenuLabel`, `MenuSeparator`, `MenuGroup` | `components/ui/menu.tsx` | Radix DropdownMenu | Roving focus, typeahead, Escape, focus return and collision-aware placement. A native popover has no menu keyboard model. |
| `Toast`, `ToastProvider` (with `useToast`) | `components/ui/toast.tsx` | Radix Toast | A polite live region per toast (`type="background"`), auto-dismiss that pauses on hover and focus, F8 to reach the stack, swipe to dismiss. |

## Contract primitives (primitives workstream)

These four are in `packages/core/contract.json` and exist on every target (web, Ink, Rust). Props and state names match the contract exactly.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| `Table` | `components/Table.tsx` | native `<table>` in a labelled, focusable scroll region | Real table semantics. States default, empty, loading, error; header and a hairline stay in every state. |
| `KeyHints` | `components/KeyHints.tsx` | native `<ul>` and `<kbd>` | One line of plain key and label pairs; what does not fit is clipped from the end. |
| `Prompt` | `components/Prompt.tsx` | native `<form>` and `<input>` | States idle, busy, error. Busy says "working" in a live region and ignores input. |
| `StateBlock`, `SkeletonBar` | `components/StateBlock.tsx` | native elements | States empty, loading, error: a word, one sentence, at most one action. Loading adds a skeleton that only fades. |

## Terminal views

All live in `terminal/TerminalWorkspace.tsx`, exposed through the public
`@ejm/ui/components/TerminalWorkspace` alias and root exports. Import
`@ejm/ui/styles-terminal.css` after `@ejm/ui/styles.css`. Add
`@ejm/ui/styles-static.css` if the page also uses static parts. These web
presentation components are separate from the terminal renderer and the
cross-target contract.
Products own authentication, pairing verification, transport, command handling
and session state. The components neither connect nor execute commands.

| Component | Primitive | Why |
| --- | --- | --- |
| `TerminalWorkspace` | native `<main>` | A full-height mono workspace. A visible `TerminalPairing` hides the log and composer through shared CSS. |
| `TerminalHeader` | native `<header>`, home link and `<h1>` | Brand, title, navigation, context and optional context actions in two quiet lines. |
| `TerminalLink` | native `<a>` | Shared compact navigation targets. `hideOnSmall` keeps secondary links out of a narrow header. |
| `TerminalOutput` | native `<section>` | Flexible output space for pairing content and the transcript. |
| `TerminalPairing` | native `<section>` | Introductory connection content. The product sets `hidden` after pairing. |
| `TerminalLog` | native `<div role="log">` | A focusable transcript with its own scroll region. `aria-live` defaults to `off` so streamed output does not interrupt input. |
| `TerminalEntry` | native `<div>` and `<pre>` | A plain-text entry. `kind` selects prompt, assistant, output or error presentation. Label text never selects a style. |
| `TerminalStatus` | native `<div>` | Connection and status on one line, with visible words. The product supplies a live status node when needed. |
| `TerminalComposer` | native `<section>`, `<form>` and `<fieldset>` | Prompt, input, actions, outcome and hints in one form. `formProps` carries standard form attributes. |
| `TerminalTextarea` | shared `Textarea` | Terminal spacing with native multiline input and a bounded maximum height. |
| `TerminalSelect` | shared `Select` | A compact native picker, with the shared coarse-pointer target. |

## Layout language (layout workstream)

Band, SectionHead, Rows, Row, Facts, Stat, Hero, ProductWindow, ProductShot, Illustration, Text, Rule, Status, Art. Add a row per component: name, file, primitive and why.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| Band | layout/Band.tsx | native `<section>` with `data-band` | A band is a data attribute and a utility, so tone needs no JavaScript. |
| Page | layout/Band.tsx | native `<div>` | The scaling content column; bands stay full-bleed around it. `variant="application"` adds 32px phone / 48px desktop vertical padding and shared content gaps; the default remains column-only. |
| SectionHead | layout/SectionHead.tsx | native header, heading and support copy | Kicker, display heading and copy. The page, section and application purposes select complete shared recipes. The application purpose stacks body-sized copy. Heading level remains semantic. |
| Rows | layout/Rows.tsx | native `<ul>` | Hairline list that replaces cards. |
| Row | layout/Rows.tsx | native `<li>` and `<a>` | The whole row is the link through its title; states per contract. |
| ActionRows | static/ActionRows.tsx | native `<ul>` | Compact hairline records whose actions stay separate from their text. |
| ActionRow | static/ActionRows.tsx | native `<li>` | Title, description, mono metadata, actions and optional child content. Text and actions wrap independently. |
| DocumentContent | static/DocumentContent.tsx | native `<div>` | Shared reading width and typography for documents, native forms and preformatted output. |
| Facts | layout/Facts.tsx | native `<dl>` | Mono labels with hairlines; stacks by container width. |
| Fact | layout/Facts.tsx | native `<div>` with `<dt>` and `<dd>` | One label and value pair inside Facts. |
| Stat | layout/Stat.tsx | native `<p>` pair | Large numerals with a mono label and no box; states per contract. |
| Text | layout/Text.tsx | native span or element | Default, muted, strong and code variants per contract. |
| Rule | layout/Rule.tsx | native `<hr>` or labelled rule | A single hairline, never a frame. |
| Status | layout/Status.tsx | native `<span>` | Glyph and word, so status never relies on color. |
| Hero | layout/Hero.tsx | native `<section>` | Type-led opening on the page ground with an aside slot for windows. |
| Art | layout/Art.tsx | inline `<svg>` | Decorative symbol line art; draw-on is added after mount only. |
| ProductWindow | layout/ProductWindow.tsx | native `<figure>` | Floating window with elevation and the Simulated screen label unless live. |
| ProductStage | layout/ProductWindow.tsx | native `<div>` | Layers one to three windows; a tucked stack on narrow stages. |
| ProductShot | layout/ProductShot.tsx | native `<figure>` and `<picture>` | A real product capture: AVIF, WebP and PNG sources, explicit size, dark and light variants that follow the page theme, a hairline caption from the capture metadata. |
| Illustration | layout/Illustration.tsx | native `<figure>` | A drawn stand-in for a product screen (a React composition) with the always-visible label "Illustration of <product>, not a screenshot". Never a capture. |
| IllustrationLabel | layout/Illustration.tsx | native `<p>` | The one visible label "Illustration of <product>, not a screenshot", shared by Illustration and ProductShot. |

## Opinionated site composition

Sites supply content, routes, artwork data and accessible state. These components own their full visual recipes. They do not accept `className` or `style`. Add missing layouts here before using them in a site.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| SectionBody | layout/SiteContent.tsx | native elements | Section content spacing. |
| Columns | layout/SiteContent.tsx | native elements | One explanatory and one supporting column. |
| MetricGrid | layout/SiteContent.tsx | native elements | Comparable figures in one responsive grid. |
| ContentFlow | layout/SiteContent.tsx | native elements | Readable content and status rhythm. |
| ContentGroup | layout/SiteContent.tsx | native elements | A heading, copy and related action. |
| ReadingColumn | layout/SiteContent.tsx | native elements | Bounded source example width. |
| Actions | layout/SiteContent.tsx | native elements | Wrapping standalone actions. |
| Note | layout/SiteContent.tsx | native elements | A nearby qualification. |
| ContentHeading | layout/SiteContent.tsx | native elements | A secondary heading with semantic level. |
| InlineLink | layout/InlineLink.tsx | native elements | Shared route and external link treatment. |
| InlineCode | layout/SiteContent.tsx | native elements | Short code inside prose. |
| CommandLines | layout/SiteContent.tsx | native elements | Plain source commands without a client highlighter. |
| VisuallyHidden | layout/SiteContent.tsx | native elements | Accessible names without visual text. |
| FileDetail | layout/SiteContent.tsx | native elements | Wrapping release metadata. |
| FocusRegion | layout/SiteContent.tsx | native elements | A programmatic focus target with the global focus rule. |
| AssetGrid | layout/SiteContent.tsx | native elements | Uniform responsive artwork grid. |
| BrandAsset | layout/SiteContent.tsx | native elements | Fixed-height artwork preview and caption. |
| PaletteSwatch | layout/SiteContent.tsx | native elements | Approved token color sample. |

## Shell, hooks and layouts (shell workstream)

Header, Footer, ThemeToggle, ExternalLink, SiteLayout, DocsLayout, AppShell, SkipLink, Collapse, Reveal, Stagger. Add a row per component.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| SiteLayout | shell/SiteLayout.tsx | native landmarks | Skip link, header, `main#main` and footer for every site. |
| Brand | shell/Brand.tsx | decorative mark and optional name | Supplied light/dark image URLs and intrinsic dimensions; placement owns the display size. The footer always uses its light artwork. |
| Header | shell/Header.tsx | native `<header>`, `<nav>` and `<details>` | Logo, nav and the theme control last; `themeControl` replaces or suppresses the default. The phone menu is a disclosure. |
| Footer | shell/Footer.tsx | native `<footer>` | Ink band with link columns and the privacy note slot. |
| ThemeToggle | shell/ThemeToggle.tsx | native `<button>` | Far right of the header; its name states the action. |
| ExternalLink | shell/ExternalLink.tsx | native `<a>` | `rel="external noopener"` with an arrow and a screen reader note. |
| SkipLink | shell/SkipLink.tsx | native `<a>` | First focusable element; targets `#main`. |
| DocsLayout | shell/DocsLayout.tsx | native `<details>` collapses | Sidebar, 68ch reading column and contents; both side parts fold on narrow screens. |
| AppShell | shell/AppShell.tsx | native landmarks | Top bar, side nav and content for applications. `themeControl` replaces or suppresses the default. |
| Collapse | shell/Collapse.tsx | native `<details>` | Shared disclosure behind the docs sidebars. |
| Disclosure | static/Disclosure.tsx | native `<details>` and `<summary>` | Always collapsible at every width, with the platform open/closed marker. Section presentation uses hairlines; compact menu presentation opens a bounded overlay. No scripts required. |
| LinkProvider | shell/link.tsx | React context | Lets a site pass its router link, so the shell stays router-agnostic. |

## Docs components (docs workstream)

CodeBlock, CodeTabs, Callout, ParamTable, Pager, SidebarFilter, Contents. Add a row per component.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| DocsSidebar | docs/DocsSidebar.tsx | native `<nav>` and lists | Grouped docs navigation with `aria-current`; hairlines only. |
| SidebarFilter | docs/SidebarFilter.tsx | native `<input>` | Filters entries and announces the result count. |
| Contents | docs/Contents.tsx | native `<nav>` and links | On this page; works with no script, scroll-aware with it. |
| Heading | docs/Heading.tsx | native heading with anchor | Heading with an id and a native `#` permalink. |
| CodeBlockFrame | docs/CodeBlockFrame.tsx | copy and feedback | Live code figure that accepts pre-rendered source. |
| CodeSource | docs/CodeBlockFrame.tsx | source region | Labelled, keyboard-scrollable plain or highlighted code. |
| CodeHighlight | docs/CodeHighlight.tsx | syntax source | Highlighted source without its figure or copy control. |
| CodeTabsFrame | docs/CodeTabsFrame.tsx | tabs and keyboard controls | Shared tabs interaction around supplied content. |
| CodeBlock | docs/CodeBlock.tsx | native `<pre>` and `<button>` | Language label, copy with a live confirmation, own scroll region. |
| CodeTabs | docs/CodeTabs.tsx | core tabs model | Arrow-key language tabs; all panels show stacked without script. |
| Callout | docs/Callout.tsx | native `<aside>` | Note, tip or warning with a text label first and a left rule, no box. |
| ParamTable | docs/ParamTable.tsx | native `<table>` | Mono names and tabular numerals, scrolling inside its own region. |
| Pager | docs/Pager.tsx | native `<nav>` and links | Previous and next links that name the adjacent pages. |
| DocHead | docs/model.tsx | native header | Page title and lead from the content model. |
| DocsPage | docs/DocsPage.tsx | live docs shell | Sidebar, contents, reading surface, code controls and pager around optional static prose. Code renders as plain source when no server highlighting callback is supplied, including inside a static boundary. |
| Prose | docs/Prose.tsx | reading surface | Shared body type and inline-code rules for docs content. |
| DocTable | docs/DocTable.tsx | native table | Captioned, locally scrolling table that preserves every column. |
| DocProse | docs/DocProse.tsx | compiled prose | Renders a serialized prose run with the shared docs parts. |
| DocCodeBlock | docs/DocCodeBlock.tsx | compiled code | Live code and tabs without the prose renderer. |
| DocDataBlock | docs/DocDataBlock.tsx | compiled block | Renders serialized blocks, including nested code. |
| DocBlocks | docs/model.tsx | content blocks | Renders blocks from the React content model. |
| DocBody | docs/model.tsx | native article content | Renders a page's blocks from the content model. |

## App (app workstream)

All in `packages/ui/src/app`. Every part takes `state` default, empty, loading or error with one sentence and at most one action, built on `StateBlock`, `SkeletonBar` and `Table`.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| `Dashboard` | `app/Dashboard.tsx` | `section`, `Stat`, `Table`, container queries | Figures, a chart, a hairline table and an activity feed with no boxes, one column on a phone and one editorial row on an ultrawide. Loading speaks once and holds placeholders `inert`. |
| `ChartLine`, `ChartArea` | `app/ChartLine.tsx` | inline SVG with `role="img"` | Dependency-free. Six hue roles plus marker shapes, dash patterns and end labels, so values never rely on color alone. |
| `ChartBar` | `app/ChartBar.tsx` | inline SVG with `role="img"` | Grouped bars with a fill pattern per series and value labels where they fit. |
| `ChartFrame` | `app/ChartFrame.tsx` | `div role="group"`, `figure`, native `details` | Shared heading, legend, "View as table" disclosure (always in the DOM) and states for the charts. |
| `Legend` | `app/Legend.tsx` | native `ul` | A text legend naming every series next to its marker and dash or pattern. |
| `XAxis`, `YAxis` | `app/axes.tsx` | SVG `g` and `text` | Hairline gridlines and tabular mono labels, thinned on narrow charts. |
| `Marker`, `BarPatterns` | `app/marks.tsx` | SVG shapes and `pattern` | The non-color encodings: six marker shapes and six bar fill patterns. |
| `ActivityList` | `app/ActivityList.tsx` | native `ul`, `time`, `Status` | Hairline rows with relative times and a status glyph and word. |

## Compact density (applications)

Application page layout is opt-in and independent of control density. Use
`Page variant="application"` with `SectionHead purpose="application"`. Native
forms and content groups use the fixed public `AppForm`, `AppField`,
`AppFieldset`, `AppFlow`, `AppGroup` and `AppActions` recipes from
`@ejm/ui/layout/Application`. The consumer keeps semantics, values, validation
and events. The shared owner fixes spacing, widths and responsive geometry.
`GameWorkspace`, `PlayerStrip`, `MovePairs` and `MoveHistory` apply the same
boundary to board composition and move selection. See `src/layout/README.md`
for the complete pattern and site design rules.


## Application leaves

These exports supply their complete visual treatment. Applications use content,
state and semantic attributes without visual classes or inline styles.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| GameClock | components/GameClock.tsx | native span | Supplied time and state, no timer or ticking live region. |
| RadioGroup | components/ui/radio-group.tsx | native fieldset and radio inputs | Form validation, submission and keyboard selection. |
| Disclosure (deep controls import) | components/ui/disclosure.tsx | native details and summary | Preserves mounted children while closed. |
| Prose | layout/Prose.tsx | native article or div | Shared reading typography without a highlighter. |

## Application composition

All exports below are public through `@ejm/ui/layout/Application`. They accept
content, refs, native events and semantic attributes without visual overrides.
Form and game modules remain separate so account routes do not load replay UI.

| Component | File | Primitive | Why |
| --- | --- | --- | --- |
| AppFlow, AppGroup | layout/Application.tsx | semantic content tags | Shared application rhythm and safe wrapping. |
| AppActions | layout/Application.tsx | semantic action or navigation region | Wrapping actions without consumer spacing choices. |
| AppHeading | layout/Application.tsx | native h2, h3 or h4 | Fixed type with heading levels and programmatic focus. |
| AppField, AppFieldset, AppForm | layout/Application.tsx | native form elements | Labels, legends, validation and submit handlers stay native. |
| AppSection | layout/Application.tsx | native section | Fixed reading measure for account and information pages. |
| AppPre | layout/Application.tsx | native pre | Wrapped source and records without document overflow. |
| GameWorkspace | layout/ApplicationGame.tsx | section and aside | Board and related state share fixed desktop and phone geometry. |
| PlayerStrip | layout/ApplicationGame.tsx | native content row | Player details align with the supplied clock. |
| BoardPlaceholder | layout/ApplicationGame.tsx | square loading region | Stable space while the lazy board loads. |
| MovePairs | layout/ApplicationGame.tsx | native ordered list | Accepted moves remain readable in a bounded region. |
| MoveHistory | layout/ApplicationGame.tsx | ordered list and shared Buttons | Current replay step, selection callback and list-only scrolling. |

## Control density

`Button`, `Switch`, `Tooltip`, `Badge`, `StateBlock`, `Table` and `NativeCheckbox` take `density="compact"` (default `comfortable`, every existing default is unchanged). Compact is for dense application UI such as the IDE: the 5px `rounded-compact` radius and 13px type; a button is 32px tall on a fine pointer whatever its `size` (`icon` is 32px square); a switch has a 28 by 16px track with a 32px target; a tooltip sets 13px mono text; a badge is 12px; a state block uses less space; a table has tighter rows. `NativeCheckbox` sizes its native input to 16px in compact density and 24px in comfortable density; the wrapping label is the target. `Disclosure` accepts the same density, defaulting to compact for a menu and comfortable for a section. On a coarse pointer (touch) a compact control is still 44px: `touch-target` makes it 44px on coarse pointers and `compact-target` makes it 32px on fine ones (previously measured in Chrome for a button, an icon button and a switch; new consumer layouts need their own browser check). `Table` also takes `truncate={false}` to show every character of a cell. `Button` gains `variant="danger"` and `size="icon"` in both densities.

## Native static pages

These are source APIs. Installing, rendering or testing them does not publish
the package, deploy a site or establish real account and session behavior.

Use public deep imports to keep dependency selection clear:

```tsx
import { NativeCheckbox } from '@ejm/ui/components/NativeCheckbox';
import { Disclosure } from '@ejm/ui/shell/Disclosure';
import { ActionRows, ActionRow } from '@ejm/ui/layout/ActionRows';
import { DocumentContent } from '@ejm/ui/layout/DocumentContent';
import { TerminalWorkspace, TerminalEntry } from '@ejm/ui/components/TerminalWorkspace';
```

The same names and their public prop types are available from `@ejm/ui`.
The static parts live in `src/static`. The deep paths above are public aliases.
Import `@ejm/ui/styles.css` once. Add each partial required by the page:

```css
@import "@ejm/ui/styles.css";
@import "@ejm/ui/styles-static.css";   /* NativeCheckbox, Disclosure, ActionRows, DocumentContent */
@import "@ejm/ui/styles-terminal.css";
```

Static and terminal styles stay opt-in so unrelated sites do not ship them.
A page that uses both groups needs both partials. Neither group needs the docs
or app stylesheet. Both groups share the base theme. A consumer can prerender
the components to HTML without a frontend runtime in the browser.

### Native forms and disclosures

- `NativeCheckbox` forwards standard input attributes, including `name`,
  `value`, `required`, `disabled`, `form`, `checked`, `defaultChecked` and
  `aria-describedby`. `className` applies to the input. `label` creates visible
  label text; otherwise provide an accessible input name. A required checkbox
  blocks native submission until selected. It has no mixed-state prop; use
  `Checkbox` when you need the Radix indeterminate contract.
- `Disclosure` takes `label`, `presentation="section"` or `"menu"`, optional
  `density`, `summaryClassName`, `contentClassName` and standard details props
  such as `open`, `name` and `onToggle`. Keep links and buttons in its content,
  outside the summary. A menu has ordinary disclosure keyboard behavior. Use
  `Menu` when you need arrow keys, typeahead, Escape, focus return or collision
  handling. `Collapse` remains the breakpoint-based sidebar component.
- `Header` and `AppShell` take `themeControl?: ReactNode`. Omitted or
  `undefined` keeps `ThemeToggle`; `null` suppresses it. A replacement can be a
  shared `Button` whose behavior an external consumer script attaches. Default
  `ThemeToggle` still needs hydration to change the theme.
- `ActionRows` forwards native list attributes. `ActionRow` adds visible
  `title`, optional `description`, `meta`, `actions` and `children`; its `title`
  is not the native tooltip attribute. Put shared buttons, links or forms in
  `actions`. The row does not turn its full area into a link. Give each action
  a name that identifies the affected record.
- `DocumentContent` sets reading width, headings, form rhythm, wrapped `<pre>`
  text and status/error presentation. Compose it from semantic headings,
  sections, labels and shared controls; it does not validate forms or parse
  content.

### Terminal composition and native updates

Compose a terminal page in this order: `TerminalWorkspace`, `TerminalHeader`,
`TerminalOutput` containing `TerminalPairing` and `TerminalLog`,
`TerminalStatus`, then `TerminalComposer`. `TerminalHeader` requires
`homeHref`, `homeLabel`, `brand` and `title`; its `navigation`, `context` and
`contextActions` slots are optional. `TerminalComposer` requires `legend`,
`prompt`, `input` and `actions`; use an actual label linked to the input in
`prompt`. `hints`, outcome children and native `formProps` are optional. Do not
nest this workspace inside another `main` or `AppShell`.

Transcript body text is 16px on phones and 14px from the `sm` breakpoint.

`TerminalLog` has `tabIndex=0` and `aria-live="off"` by default; give it an
accessible name. For changing connection words, supply a separate
`role="status"` node to `TerminalStatus`. `TerminalEntry` takes a string label,
plain string children and `kind="prompt" | "assistant" | "output" | "error"`.
New entries do not announce every streamed character.

A native consumer can prerender a `TerminalEntry` inside a `<template>`, clone
it, and fill `[data-terminal-label]` and `[data-terminal-text]` with
`textContent`. Set `data-kind` to one of the public entry kinds. Never inject
transcript text through `innerHTML`, infer kind from a translated label or
copy the component's class list. The product sets `hidden` on
`[data-terminal-pairing]` after verified pairing and controls form enablement.
These presentation hooks do not verify a host or authorize a command.

Use an external script for consumer handlers under a strict content security
policy. These components emit no inline scripts or styles by default. Check
the actual HTML, emitted CSS, focus, coarse-pointer targets, overflow and
forced-colors behavior in the consuming product before release.
