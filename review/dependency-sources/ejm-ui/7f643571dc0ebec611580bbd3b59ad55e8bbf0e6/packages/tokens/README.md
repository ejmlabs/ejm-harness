# @ejm/tokens

The single source for ejmlabs color and brand: `theme.json` (the ui theme and the web bands), the brand palette and the assets in `assets/` (each file listed with its sha256 in `assets/PROVENANCE.md`).

```bash
npm run build -w @ejm/tokens   # regenerate everything below from theme.json
npm run check -w @ejm/tokens   # contrast gates, and fail on any stale generated file
```

| Output | For |
| --- | --- |
| `generated/tailwind.css` | Tailwind v4 theme: brand colors, semantic colors, the four bands, elevation. Imported by `@ejm/ui/styles.css`. |
| `generated/window.css` | The ui roles and syntax colors as web variables and Tailwind colors, for product windows and editors. |
| `generated/neutral.css` | The complete neutral scale for native product styles on the web, through the opt-in `@ejm/tokens/neutral.css` export. It follows the page theme and adds no utilities. |
| `generated/theme.css` | The ui roles as plain `--ejm-ui-*` variables, dark by default, for the IDE and other non-Tailwind apps. |
| `generated/monaco/*.json` | Monaco editor themes. |
| `generated/terminal/*` | Color schemes for iTerm, Warp, Alacritty, kitty, Ghostty and Windows Terminal. |
| `rust/ejm_theme.rs` | The ratatui palette. `rust/ejm_tui.rs` and `rust/TUI.md` sit beside it, hand written. |

Edit `theme.json`, run the build, commit the generated files. Names and behavior of the web theme are documented in `packages/ui/THEME.md`.

## What the npm tarball holds

The package includes the retained brand assets under `assets/`: SVGs, raster variants, editable sources, templates, original TTF and WOFF2 fonts and their notices. Provenance lists each import. ZIP archives stay out of the tarball; Studio assembles public downloads from these named inputs.

Use the public `@ejm/tokens/assets/*` export. Project families live in `assets/projects/<id>/`; ejmlabs uses the root sets. The npm file list includes every retained non-ZIP asset, subject to its license. `rust/ejm_tui.rs` and `rust/TUI.md` are included separately from brand assets.

## Asset maintenance and rights

From the repository root, run `node scripts/assets.mjs --local` to verify retained files against [their provenance](assets/PROVENANCE.md). It checks local hashes and inventory without the old kit. The optional `--sync` import uses `EJM_DESIGN_KIT`, or the legacy `../ejmlabs/design-kit` directory. That legacy source is not a build dependency.

Legacy imports exclude retired social profile kits and ZIP archives. CLI, IDE and Harness retain the approved editable logo SVGs; other project editable sources stay in the kit. Editable lockups and templates already in provenance are retained and packaged. The table and import filter in `scripts/assets.mjs` define this inventory. Do not remove table rows without removing their files and checking affected consumers.

### Relay assets

Relay's 28 production assets are available through the existing
`@ejm/tokens/assets/projects/relay/*` export. They include marks, outlined logos,
square icons, browser icons, a manifest and social cards. The retained kit owns
the artwork source and local review materials. Its `editable/` sources,
`README.md`, `index.html` and `review.svg`/`review.png` are not imported here.

To refresh only Relay from a reviewed kit, run from this repository root:

```sh
EJM_DESIGN_KIT=/path/to/ejmlabs/design-kit node scripts/assets.mjs --sync --project relay
node scripts/assets.mjs --local
```

This targeted import updates only Relay files and their provenance rows. It
preserves all other rows and the complete moved-and-edited source record. It
fails before copying files if a required Relay source is missing. A plain
`--sync` still refreshes the full retained import set. Run the normal repository
gate before release. Importing artwork does not deploy a consumer or publish a
package.

Public consumer examples:

```js
import.meta.resolve('@ejm/tokens/assets/projects/relay/mark-ink.svg');
import.meta.resolve('@ejm/tokens/assets/projects/relay/favicon.svg');
```

### Rights

Brand materials retain their owner's rights. Manrope and IBM Plex Mono retain the SIL Open Font License notices in `assets/fonts/`. See the package [license](LICENSE) and [third-party notices](THIRD_PARTY_NOTICES.md) before copying assets or fonts.

## Public brand downloads

The `@ejm/tokens/assets/*` export and package file list include the curated brand
inputs used by Studio: named project logos and marks, editable SVG wordmarks,
family artwork, raster variants, original font files/notices and templates.
The retained provenance table records their hashes. ZIP archives are assembled
by Studio and are not retained in this package. Public source access does not
grant trademark rights; preserve the terms and original notices in LICENSE and
THIRD_PARTY_NOTICES.md. Rebuild and check the intended archive against the actual
package file list before publishing it.

Run `npm run pack:check -w @ejm/tokens` to check that every retained public asset
is included by npm's package file list. Run `npm pack -w @ejm/tokens` and compare
the actual tarball against the consumer's named download set before release.
Retired social kits and Studio-owned swag masters are not restored by this
contract. Active UI colors still come from `theme.json` and its generated files.
