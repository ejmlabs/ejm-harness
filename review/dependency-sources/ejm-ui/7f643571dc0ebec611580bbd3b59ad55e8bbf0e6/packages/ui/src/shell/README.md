# Shell

Header, footer, layouts and the external link. No cards: the header, the nav and the footer are the page ground, a band, or hairlines. The only shadow is the open phone menu, which is an overlay.

| Export | Purpose |
| --- | --- |
| `SiteLayout` | Skip link, `Header`, `<main id="main" tabIndex={-1}>`, `Footer`. The footer sits at the bottom of short pages. |
| `Brand` | Decorative mark and optional name, intrinsic proportions, optional dark variant, and fixed navigation, project or footer placement. The surrounding link or heading supplies the accessible name. |
| `Header` | Sticky, on the page ground (an opaque `band`, so text never sits on page content while scrolling; `band` sets a matching ground when the page opens on Mist, Sage or Ink), no box or rule. Structured `identity` at the left. Nav, then `ThemeToggle` last at the far right. Below 768px the nav is one native `<details>` Menu. |
| `Footer` | Ink band with structured `identity`, link columns, a `privacy` slot, a `legal` line, and a safe-area bottom inset. |
| `ThemeToggle` | Sun and moon, 44px, named for its action. Uses `useTheme`. |
| `ExternalLink` | `rel="external noopener"`, an arrow, and ", opens another site" for screen readers. |
| `SkipLink` | `<a href="#main" class="skip-link">`. Included by `SiteLayout` and `AppShell`. |
| `DocsLayout` | Grid for a docs page: `sidebar`, `contents` and the article in a 68ch column. Put it inside `SiteLayout`. |
| `AppShell` | Top bar (`actions`, then the theme control), side `nav` and content, for applications. A whole page frame with its own skip link and `main`. |
| `Collapse` | The shared disclosure behind the sidebars: one `<details>`, labelled on narrow screens, always visible from a breakpoint. |
| `Disclosure` (root or deep import) | Native `<details>` with a `label`, always collapsible at every width. Default `presentation="section"` uses hairlines in normal flow; `presentation="menu"` opens a bounded, scrollable overlay. |
| `LinkProvider`, `useLinkComponent`, `LinkComponent`, `NavItem` | Router-agnostic links. |

## Links without a router

Internal links use a `LinkComponent` (default: a plain `<a>`). Layouts take `linkComponent` and provide it to everything below, so `Header`, `Footer` and anything calling `useLinkComponent()` agree.

```tsx
const RouterLink: LinkComponent = ({ href, ...rest }) => <Link to={href} {...rest} />;
<SiteLayout header={...} footer={...} linkComponent={RouterLink}>...</SiteLayout>
```

External items (`external: true`) always render as a plain `ExternalLink`.

## Theme control slot

`Header` and `AppShell` accept `themeControl?: ReactNode`. Omit it or pass
`undefined` to keep `ThemeToggle`. Pass `null` to suppress the control. A native
page can supply a shared `Button` and attach its handler from an external
consumer script. The default toggle still needs hydration to change the theme.

## Disclosures at every width

Import `Disclosure` from `@ejm/ui/shell/Disclosure` or `@ejm/ui`. It accepts
standard details attributes and `label`, `presentation`, `density`,
`summaryClassName` and `contentClassName`. A menu defaults to compact density;
a section defaults to comfortable. Both keep the coarse-pointer target.

The implementation lives in `src/static/Disclosure.tsx`. Import
`@ejm/ui/styles-static.css` after `@ejm/ui/styles.css`. The shell deep path is
a public alias; the `@ejm/ui/shell` barrel does not export this static part.

Keep links and controls in the disclosed content, outside its summary. Native
opening and closing work before hydration, with the platform's open/closed
marker. Menu presentation does not add
arrow-key navigation, Escape, focus return or collision handling; use `Menu`
for that interaction contract. `Collapse` keeps its existing breakpoint
behavior. None of these APIs connects a product session or changes an account.

## Narrow screens, with no script

- `Header`: the Menu is a native `<details>`. Closed, it takes no space. Open, the nav is an absolute overlay under the bar, so nothing moves. A small effect adds Escape (focus returns to the button), a press outside, and closing after a link is chosen.
- `DocsLayout` and `AppShell` render the sidebar once, in a `<details>` that is closed on narrow screens in the server HTML. From the breakpoint up (1024px sidebar, 1280px contents, 768px app nav) the button is hidden and `::details-content` is forced visible, so there is no duplicate markup and no script. Browsers without `::details-content` keep the button (it is hidden only where the browser supports that pseudo-element, through `supports-[selector(::details-content)]`) and get a small effect that opens the details on wide screens, so the content is reachable with or without script.
- Source order in `DocsLayout` is sidebar, contents, article, which is also the phone order.

## Accessibility notes

- Landmarks: `header` (banner) with one `nav` named "Main" (the list above 768px, the Menu disclosure below, never both in the accessibility tree), `main#main` with `tabindex="-1"`, `footer` (contentinfo) with one `nav` named "Footer" whose column headings name the groups, and the docs navs "Documentation", "On this page" and "Previous and next pages". Every name is unique, which `shell.test.tsx` checks with axe on a whole docs page. A router can move focus to `main` after navigation. Each page has one `h1`.
- `aria-current="page"` marks the current link, with a bold underline as well, so it is not color alone.
- The toggle's name changes with its state ("Switch to dark theme"), and a polite status region says the result. It does not use `aria-pressed`, so name and state never disagree.
- Targets are 44px: the toggle always, links and the Menu button through `touch-target` on coarse pointers. Contrast was checked by script in both themes for header, footer, nav and app shell.
- Native `<details>` is used instead of a Radix collapsible: it works without script and needs no state.

## Shared identity

`Brand placement="project"` inherits its heading type from `Rows` or `SectionHead` and keeps the shared project mark size. The containing heading controls the name size.

`Header` and `Footer` accept `identity={{ name, artwork, description }}`. `name` is plain text. `artwork` is optional and contains `src`, optional `darkSrc`, and intrinsic `width` and `height`. Supply the standalone mark when also showing a name. The footer uses `darkSrc` on its permanent Ink ground in either page theme. Both shell components own brand spacing, scale and type. Authored sites use this identity contract. Legacy non-site component consumers retain `logo`, `brand` and `className`; the site guard rejects these visual overrides.
