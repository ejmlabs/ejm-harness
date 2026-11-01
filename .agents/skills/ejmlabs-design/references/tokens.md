# Tokens

The names an agent reaches for most. The full list, with every value per band and theme, is
`THEME.md` in the installed `@ejm/ui` package. If a name is not there, it does not exist.

## Palette

| Brand  | Hex       | Use                                                       |
| ------ | --------- | --------------------------------------------------------- |
| Sage   | `#8A9A7B` | The lead color, about 30 percent of a page                |
| Forest | `#29443B` | Links and accents, under 10 percent, never a large ground |
| Ink    | `#202721` | Text in light, the dark band in either theme              |
| Paper  | `#F6F5EF` | The page ground in light                                  |
| Mist   | `#E8EDE3` | The quiet alternate band                                  |

Sage on Paper is 2.75:1, so Sage is never small text on Paper. A Sage button is identified by
its Ink label; add `border-control` when a control needs a visible edge.

## Semantic names

| Name                                                    | Meaning                                                                    |
| ------------------------------------------------------- | -------------------------------------------------------------------------- |
| `bg-ground`, `text-fg`                                  | The band background and its body text                                      |
| `text-muted`                                            | Secondary text, still 4.5:1                                                |
| `border-line`, `divide-line`                            | Hairline rules, not control edges                                          |
| `bg-tone`                                               | Hover and selected row fill, not a box                                     |
| `bg-field`                                              | Input background                                                           |
| `text-link`, `text-link-large`                          | Link and accent text; `-large` only for 24px and up                        |
| `border-control`                                        | Input and outline button borders, 3:1                                      |
| `ring-focus`, `outline-focus`                           | Focus ring, 3:1                                                            |
| `bg-primary`, `text-on-primary`                         | Primary button fill and its text                                           |
| `text-success`, `text-warn`, `text-danger`, `text-info` | Status text, always with a word or glyph                                   |
| `hue-1` to `hue-6`                                      | Chart series; not defined on Sage                                          |
| `ui-*`, `syn-*`                                         | Product windows, editors and syntax; follow the page theme, never the band |

There is no `card` color and no `bg-muted`; `muted` is a text color.

## Bands

`<section data-band="paper|mist|sage|ink" class="band py-rhythm">`, with a `page` column inside.
Bands may nest; the nearest wins. Children use only semantic names, so they read on every ground.

- Paper is the page. Mist is the alternate. Sage and Ink keep their ground in both themes.
- On Sage all text is Ink, including muted and links. Build hierarchy from size, weight and the
  mono face, not a lighter color. Forest on Sage is for large type only. Charts never sit on Sage.
- `scheme-page` keeps a product window or code well in the page theme inside a Sage or Ink band.

## Type

| Utility                      | Use                                                     |
| ---------------------------- | ------------------------------------------------------- |
| `text-display`               | The one display line on a page, and a standalone number |
| `text-title`, `text-heading` | Section and subsection headings                         |
| `text-lead`                  | The opening paragraph                                   |
| `text-body`                  | Running text, 16px on phones                            |
| `text-meta`                  | Labels and metadata, IBM Plex Mono                      |
| `tabular-nums`               | Numbers in tables, facts rows and versions              |

The scale is six sizes: display, title, heading, lead, body and meta. Controls and dense tables
use `text-base` (16px), or `text-sm` (14px) when compact; the kit uses no other Tailwind size.
Body lines stay within `max-w-measure` (68ch). Headings wrap balanced; prose wraps pretty.
Preload both fonts with `fontPreloads` from `@ejm/ui`.

## Space, radius and elevation

- Column: `page`. Vertical rhythm: `py-rhythm` and `*-rhythm-sm`. Gutters: `px-gutter`.
- Groups: `gap-field` (label, control, help) and `gap-content` (app content and forms).
- Radius: `rounded-control` (6px) for controls, `rounded-pill`, `rounded-window` (16px) for
  product windows, menus and overlays. No card or panel radius exists.
- Elevation: `shadow-rest`, `shadow-raised`, `shadow-float`, for windows, menus and overlays only.

## Motion

| Token                                             | Value                             |
| ------------------------------------------------- | --------------------------------- |
| `duration-fast`, `duration-base`, `duration-slow` | 120ms, 200ms, 320ms               |
| `ease-standard`                                   | State changes                     |
| `ease-enter`, `ease-exit`                         | Things arriving and leaving       |
| `ease-emphasis`                                   | The one hero entrance and draw-on |

- `animate-fade`, `animate-reveal`, `animate-enter`, and `animate-slide` (a rise with no fade, for
  text that can be the Largest Contentful Paint).
- Stagger with `--ejm-i` on each child. The index stops at 8, so an entrance stays under 1.2s.
- Every animation utility is wrapped in `prefers-reduced-motion: no-preference`.

## Themes

- `theme-boot.js` runs before any CSS, so the first frame has the right theme. Without a saved
  choice the system preference decides.
- The theme control sits last in the header tools. `?theme=dark` and `?theme=light` are test
  overrides and are never saved.

## Terminal interfaces

- Build from `ejm-tui` blocks. No boxes, frames or painted backgrounds; group with space and one
  hairline rule. Every state has a word or glyph and works under `NO_COLOR`.
