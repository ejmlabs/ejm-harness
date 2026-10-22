# Layout

The containerless language as components. Group with display type, space, hairline rows and full-bleed bands. There is no card, panel or tile here, and only `ProductWindow` has a shadow (and `ProductShot` when you ask for `elevation`). Import from `@ejm/ui`. Colors and sizes come only from `packages/ui/THEME.md`.

| Component | Props | Use it for | Accessibility |
| --- | --- | --- | --- |
| `Band` | `tone` paper, mist, sage or ink (default paper); `as`; `column` (default true, wraps children in `Page`); `flush` (no vertical rhythm) | A full-bleed section with its own ground. Children read correctly on every band. Alternate tones by section, and let Sage lead (about 30 percent of a page). | Pass `aria-labelledby` to name a section. |
| `Page` | `variant` default or application; div props | The content column. The application variant adds 32px vertical padding below 640px, 48px from 640px, and shared content gaps. The default adds no vertical padding or layout. | None needed. |
| `SectionHead` | `kicker` ("01 / Projects"); `title`; `children` (supporting copy); `purpose` page, section or application; `as` h1, h2 or h3; `id` | The opening of a section. The page purpose uses the display heading, section uses title type, and both place lead copy beside the heading from 48rem of their own width. Application stacks title and body-sized copy. The default is section. | Keep heading levels in order. Give the heading an `id` and point `aria-labelledby` at it. Do not wrap this header in another header. |
| `Rows` | `size` heading or title; `aria-label` | A hairline list that replaces cards. Rows go in columns from 42rem of the list's own width. The list reaches 1rem past the column so the tone and hairlines meet the gutter and text stays on the column edge. Use it inside `Page` or give the parent a gutter. | A `ul`. Give it a label. |
| `Row` | `title`; `description`; `meta` (mono); `href`; `external`; `download` (true or a file name); `state` default, selected, disabled or focused | One row. With `href` the whole row is the link through its title, so the link name is the title alone. Hover shifts the tone (an opacity layer) and slides the arrow (transform). `external` adds `rel="external"`, the diagonal arrow and "opens another site" for screen readers (so the link name is then "title, opens another site"). `download` passes through to the anchor. | `selected` sets `aria-current` and shows the selected glyph. `focused` shows the focused glyph and a ring (real focus uses `:focus-visible`). `disabled` renders no link and adds the contract word "(disabled)". Row targets are at least 44px on coarse pointers. |
| `ActionRows` and `ActionRow` | Native list and list-item attributes; row `title`, optional `description`, `meta`, `actions`, `children` | Compact records with separate buttons, links or forms. Long text and action groups wrap independently. | A real `ul` and `li`. Name the list and give each action a name that identifies its record. The row is never a stretched link. |
| `DocumentContent` | Native div attributes | Reading width, document headings, form rhythm, wrapped `pre` text and status/error presentation. | Compose semantic headings, sections, labels and shared controls. It adds no landmark or validation behavior. |
| `Facts` and `Fact` | `Fact label`, children | Mono labels, hairlines, values. Stacks below 28rem of its own width, so it never clips in a narrow column. | A `dl`. |
| `Stat` | `figure`; `label`; `state` default, loading, error or empty; `message`; `size` stat (default) or title | A large figure with a mono label and a hairline above. No box. Loading, error and empty replace the figure with the contract word or your message. | Loading is a polite status. Error uses `Status`, so it has a glyph and a word. |
| `Text` | `variant` default, muted, strong or code; `as` | A run of text with a variant. | On Sage, muted is already Ink: use size and weight for hierarchy. |
| `Rule` | `label` | One hairline. With `label` the mono label sits on the line. | An `hr`, or a labelled separator. |
| `Status` | `status` ok, warn, error, info or pending; `label` | A glyph and a word together, from the contract. | Never color alone: both are always present. |
| `Hero` | `eyebrow`; `title` (h1); `children` (copy); `actions`; `aside`; `art` (inline svg markup); `drawOn`; `titleId`; `footnote` | The type-led hero on the page ground. No panel. Headline across the full column, copy and actions to the left, `aside` to the right from 64rem and below the copy when narrow. Use a real `ProductShot` or a clearly labelled component example in this slot. Staged entrance with `--ejm-i` 0 to 4, so it ends 880ms after load: the eyebrow, headline and copy only slide (`animate-slide`, transform only), because the headline is the page's Largest Contentful Paint and a fade would delay it; the actions and then the aside fade and rise (`animate-enter`). Nothing is hidden without scripts and everything is final under reduced motion. | One h1. `titleId` names the region. `footnote` follows the actions. The entrance is part of the shared recipe and cannot be disabled by a site. Keep the Hero outside a server-only Static boundary. `art` is aria-hidden. |
| `Art` | `svg`; `drawOn`; `className` | Decorative symbol line art from `packages/ui/src/art/inline`. Import with `?raw`. The `draw-on` class is added after hydration only. | Always aria-hidden. |
| `ProductWindow` | `title`; `meta`; `elevation` rest, raised or float; `live`; `label`; `labelSide` | A floating product window with a title bar, three dots and a body. It keeps the page theme inside any band. Without `live` it shows the visible "Simulated screen" label (in the bar when narrow, in the footer when wide). The only layout part with a shadow. Compose its body from layout parts and only words that appear in the product README. | Hidden from assistive tech by default (decoration). Pass `label` for an interactive window and it becomes a named group. Pass `live` only for the demo islands. |
| `ProductStage` | children: one to three `ProductWindow` | Layers the windows with offset and overlap from 28rem of its own width, and tucks them into an overlapping stack below that. Window text scales with the stage and never drops under 13px. | The stage adds no semantics. |
| `ProductShot` | `shot` (from `getShot` in `@ejm/shots`); `alt` (required); `priority`; `frame`; `elevation`; `caption` (text, or `false`); `sizes` | A real capture of the real product: a figure with AVIF, WebP and PNG sources, explicit width and height, and a hairline caption naming the product, version or commit and capture date. Dark and light captures follow the page theme. Not labelled "Simulated": it is real. | Write `alt` to say what the shot shows. Lazy by default; `priority` is for the hero: eager, `fetchpriority="high"`, and the head needs `shotPreloadLinks(shot)`. `frame` draws the page's own terminal bar (dots and title) around terminal captures only and throws for a window capture. A real app window keeps its own edge, so there is no shadow unless you ask with `elevation` (rest, raised or float) for a frameless capture. |
| `Illustration` | `product`; `caption`; children (a React composition) | A drawn stand-in for a product screen where a real capture is impossible, for example the `IdeView` from `@ejm/demos`. Always shows the visible label "Illustration of <product>, not a screenshot" under it. | A figure named by that label. Never use it for a product that can be captured. `ProductShot` of a shot whose metadata says `kind: "illustration"` shows the same label and prefixes the alt text. |

## Rules of thumb

- A page has one `Hero` or `SectionHead purpose="page"` at the top and `SectionHead` for each section. Shared components own heading size and spacing.
- Do not put a `Row` list inside a bordered or filled box. If you want a box, you want a band.
- Use `Stat` for numbers that matter and `Facts` for specifications. Do not use either as a card grid.
- Product windows are objects, not groups: never wrap ordinary content in one.
- Use `ActionRows` for records with separate actions. Its `title` is visible
  content, not the native tooltip attribute. Put a real shared `Button` or a
  form in `actions`; do not copy a button class list into generated HTML.
- Use `DocumentContent` for native account, policy and setup pages instead of
  a separate typography system. It and `ActionRows` live in `src/static`.
  Import `@ejm/ui/styles-static.css` after `@ejm/ui/styles.css`. Use the root
  exports or public aliases `@ejm/ui/layout/DocumentContent` and
  `@ejm/ui/layout/ActionRows`. The `@ejm/ui/layout` barrel does not export these
  static parts.

## Application pages and forms

Use `Page variant="application"` for account, history, list and play pages.
It keeps the same column and gutters as `Page`, with 32px vertical padding
below 640px and 48px from 640px. Its direct children form a grid separated by
`gap-content`. `SectionHead purpose="application"` keeps title, optional kicker and
body-sized support copy aligned, separated by `gap-field`. The page enters once
with the shared transform-only slide, and stays still under reduced motion. Heading level
remains independent of the shared recipe.

Import the fixed application recipes from `@ejm/ui/layout/Application`.
They accept content, IDs, events, refs and accessible state. They do not accept
`className`, `style`, raw HTML or consumer spacing and width settings.

| Export | Use |
| --- | --- |
| `AppFlow`, `AppGroup` | Full-width application content or a related group, with fixed shared spacing. `as` changes semantics only. |
| `AppActions` | Wrapping actions, optionally a named navigation region. |
| `AppHeading` | Secondary heading with an independent h2, h3 or h4 level, ref and focus attributes. |
| `AppField`, `AppFieldset`, `AppForm` | Existing native labels, legends, validation and form handlers in the shared field rhythm. Forms have one reading width. |
| `AppSection` | Account and information sections at the shared reading measure. |
| `AppPre` | Wrapped source or records, including long unbroken values. |
| `GameWorkspace` | A board beside related state on desktop and above it on phones. `board`, `children` and optional `after` describe content slots. |
| `PlayerStrip`, `BoardPlaceholder` | Player and clock alignment, and a square board loading region. |
| `MovePairs`, `MoveHistory` | Accepted move pairs or selectable replay moves in a bounded scroll region. |

On desktop, `GameWorkspace` bounds the board column by viewport height as well
as the shared maximum width. This leaves room for both player strips and
clocks. A short landscape viewport keeps a playable minimum and may scroll.

`MoveHistory` takes `items`, `selectedId` and `onSelect`. Items contain `id`,
`number`, `color` and `san`. It marks the selected step and keeps it visible by
scrolling its own list. The host retains game navigation, chess rules, move
validation, clocks and network state. These recipes add no timers, game engine
or transport.

Application recipes are included in `@ejm/ui/styles.css`. The marketing-only
`styles-site.css` profile does not scan this family. Rebuild a consumer after
changing the shared owner.

## Site composition contract

Public sites use the shared `@ejm/site/vite` build plugin. It rejects local CSS, inline styles and visual class names. `Static className="contents"` is the single structural wrapper exception. Keep code highlighting and prose server-only where possible, and leave copy buttons and playback controls outside those static boundaries.

`Hero`, `SectionHead`, `Brand`, `Header` and `Footer` use fixed recipes in authored sites. The site guard rejects legacy visual props; non-site component consumers keep those props for compatibility. `Header` and `Footer` take a structured `identity`: a name, optional standalone artwork and optional description. `Brand` uses navigation, project or footer placement. Its placement selects its type, artwork height and theme behavior. Artwork dimensions describe the file's intrinsic proportions.

Import the following fixed recipes from `@ejm/ui/layout/SiteContent`:

| Export | Use |
| --- | --- |
| `SectionBody` | Space after a section opening. |
| `Columns` | `primary` explanatory content and `secondary` supporting rows or facts. The shared split is five and seven columns. |
| `MetricGrid` | Comparable `Stat` figures in one responsive grid. |
| `ContentFlow`, `ContentGroup` | Readable content rhythm, or a heading with its related copy and action. |
| `ReadingColumn` | Bound the width of source examples. |
| `Actions` | A wrapping row of standalone shared buttons. |
| `Note` | A nearby qualification in shared metadata type. |
| `ContentHeading` | Secondary heading. `as` changes its semantic level only. |
| `InlineLink`, `InlineCode` | Links and code inside sentences. Links use the public routing context. |
| `CommandLines` | A `commands` array rendered as plain source without a client highlighter. |
| `FileDetail`, `VisuallyHidden`, `FocusRegion` | Wrapping release metadata, accessible labels and programmatic focus targets. |
| `AssetGrid`, `BrandAsset`, `PaletteSwatch` | Approved artwork previews and token color samples. |

Content groups accept IDs, roles, live-region attributes and other semantic HTML attributes. They reject visual style props. Form values, validation, navigation targets, release state and product data stay in the consuming site.

When a needed pattern is missing, add it to ejm-ui with a semantic name and a complete recipe. Do not add a spacing number, arbitrary class or local exception to the site. Change a shared recipe once, rebuild each consumer, and verify its output before publishing. Installed sites receive shared changes on their next build, not as a remote theme update.

Application adapters may still compose lower-level primitives inside ejm-ui. `Page variant="application"` and `SectionHead purpose="application"` supply the shared application opening. Those internals do not permit a public site to bypass its design gate.

## Product shots

Product imagery is a real capture, not a drawn window. `packages/shots` holds the captures (`assets/<product>/<name>.<theme>.png` and `<name>.json`) and turns them into AVIF, WebP and PNG files (`node packages/shots/scripts/optimize.mjs`, checked with `--check`). A page uses them like this:

```tsx
import { getShot } from '@ejm/shots';
import { ProductShot, shotPreloadLinks } from '@ejm/ui';

const hero = getShot('cli', 'tui-read-refused');
<ProductShot shot={hero} alt="The ejm-cli terminal after a read, with a blocked write refused" priority frame />
// in the route head: { head: { title: '...', links: shotPreloadLinks(hero) } } puts the <link rel="preload" as="image"> tags in the prerendered HTML (RouteHead.links of @ejm/ssg; the same link on the site and the route is written once)
```

- A shot is either a `screenshot` (default) or an `illustration` (metadata `kind`). An illustration records no command, version, commit or transcript, and is always shown with its label. `node scripts/check-shots.mjs` rejects a shot that breaks these rules, a stale shot (warning, error with `--strict`), and a drawn `ProductWindow` or `ProductStage` used as product imagery.
- Theme: both variants are separate `<picture>`s and CSS shows the one that matches `data-theme` (or the system preference when none is set). Only the visible one downloads: the hidden one is lazy and `display: none`. For `priority` the first picture picks its variant with `<source media>` so the common visitor (theme follows the system) fetches one image, found by the preload hint; a visitor whose chosen theme differs from the system also fetches the matching variant once.
- The smaller of AVIF and WebP is listed first. Terminal text is smaller (and exact) as lossless WebP, a window capture usually as AVIF.
