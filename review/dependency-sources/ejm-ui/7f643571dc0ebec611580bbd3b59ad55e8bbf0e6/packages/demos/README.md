# @ejm/demos

Product views for ejm-cli, ejm-ide and ejm-harness: headless engines, simulated interactive islands, caller-owned recorded sources and native passive previews. Strict TypeScript, React 19. All run locally in the browser without a model connection. Recorded entries never start a provider or command.

| Import | What it is |
| --- | --- |
| `@ejm/demos` | The engines (`createCliDemo`, `createIdeDemo`, `createHarnessDemo`), the manual and system clocks, the scenario data and its types. No React. |
| `@ejm/demos/views` | `CliView`, `IdeView`, `HarnessView`: one frame of each product inside the layout `ProductWindow`. |
| `@ejm/demos/hero` | `ProductHeroStage` with `CliHeroWindow`, `HarnessHeroWindow` and `IdeHeroWindow` (pay only for the windows you pass), and `ProductHero` (all three). |
| `@ejm/demos/islands/cli-lazy`, `/ide-lazy`, `/harness-lazy` | `LazyCliDemo`, `LazyIdeDemo`, `LazyHarnessDemo`: static view first, island on approach or use. Ship only that product's view. Use these. |
| `@ejm/demos/islands/cli-lazy`, `/harness-lazy` | Additive `LazyCliShowcase` and `LazyHarnessShowcase` entries for passive, product-owned recordings. Existing interactive exports stay available. |
| `@ejm/demos/islands/cli-preview`, `/ide-preview`, `/harness-preview` | `LazyCliPreview`, `LazyIdePreview`, `LazyHarnessPreview`: native passive views with bundled, finite workflow data. No source prop. |
| `@ejm/demos/ShowcaseFrame` | `ShowcaseFrame`: fixed width and an accessible nearby disclosure, with an optional project-specific `note`. |
| `@ejm/demos/islands` | `LazyDemo product="..."`: the same for any product, but it imports all three views (about 5 KB gzip more than one). |
| `@ejm/demos/islands/cli`, `/ide`, `/harness` | The interactive islands, default exports, for `React.lazy`. |
| `@ejm/demos/styles.css` | `@source` lines so Tailwind scans the views, islands and hero (not the tests or the scenario data). Import it once from the site stylesheet, after `@ejm/ui/styles.css`. About 1.7 KB gzip of CSS. |
| `@ejm/demos/ide.css` | A stable IDE entry that imports `styles.css` for the current simulated views. Use it after `@ejm/ui/styles.css`; an existing import of `styles.css` also remains valid. |

## Static views

```tsx
import { CliView, IdeView, HarnessView } from '@ejm/demos/views';

<CliView elevation="raised" label="ejm-cli, a simulated terminal screen" />
<IdeView layout="auto" label="ejm-ide, a simulated screen" />
```

| Prop | Meaning |
| --- | --- |
| `label` | Gives the window an accessible name. Without it the window is decoration and hidden from assistive technology, as `ProductWindow` does. Pass it when the view is page content. |
| `elevation`, `labelSide` | Passed to `ProductWindow`. |
| `layout` (ide, harness) | `stack` for small windows such as a hero stage, `split` for tree, editor and chat (or governor, sessions, journal and code sample) side by side, `auto` (default) splits from the window's own width. |
| `compact` (cli) | Leaves out the status line and key hints, and fades the transcript where it is cut. The hero uses it. |
| `frame` | Another frame of the same shape. Defaults to the bundled one. |

The frames are one moment of each demo: ejm-cli proposes a patch and it is blocked because write access is off, ejm-ide has applied a change Edi proposed (the editor marks the changed lines), ejm-harness is replaying its journal after a crash. They are generated from the engines with a manual clock by `src/views/frames.test.ts` (`UPDATE_FRAMES=1 npx vitest run packages/demos/src/views/frames.test.ts` rewrites `views/frames/*.json`), and that test fails when `views/frames/*.json` stops matching the engines. A page that shows only the views therefore ships the data, not the engines or the scenario files. Code is highlighted with the docs tokenizer from `@ejm/ui` and colored with the `syn-*` window colors. A diff line starts with `+` or `-`, so it never relies on color.

## Interactive islands

```tsx
import { LazyCliDemo } from '@ejm/demos/islands/cli-lazy';   // or LazyIdeDemo from .../ide-lazy, LazyHarnessDemo from .../harness-lazy
<LazyCliDemo />                     // ships the cli static view and frame only

// one component for all three: simpler to write, but it carries all three views and frames
import { LazyDemo } from '@ejm/demos/islands';
<LazyDemo product="cli" />          // "cli" | "ide" | "harness"

// or load one yourself
const CliDemo = React.lazy(() => import('@ejm/demos/islands/cli'));
```

Each island takes the engine options (`clock`, `speed`, `reducedMotion`, `seed`) and a `scenario` to replace the bundled one. They are built on the engines with `useSyncExternalStore` and own what the engines do not model:

- Scroll-follow in the transcript, the chat and the journal (scrolling up stops it; End in the cli returns to the latest line).
- Truncation by CSS (`text-ellipsis`, `whitespace-pre`) for header, status, tool and diff lines, so nothing wraps into a mess at 320px.
- A spinner that rotates with `motion-safe` and holds still as a ring under reduced motion (an asterisk in plain mode).
- Focus: a skip link before the window, focus into an opened overlay or palette and back to where it came from on close, Escape closes an overlay first and then moves focus to the window so the next Shift+Tab leaves the demo.
- One polite `role="log"` that announces a sentence per finished step from `state.live`. Streamed characters are never announced.
- `IntersectionObserver` feeding `dispatch({ type: 'visible' })`: the demo runs only while it is in view. Where the observer is missing it runs.
- Pause, Resume and Reset in the window, the visible "Simulated" badge, and the scenario caption and note under it, always shown.
- ejm-cli: prompt chips, composer (Enter sends, Alt+Enter, Shift+Enter and Ctrl+J add a line, Up and Down recall, Ctrl+C stops, Ctrl+S opens sessions, PageUp and PageDown scroll), sessions and help overlays, Grant write and Plain switches, Stop, Sessions and Help buttons.
- ejm-ide: file list, tabs (arrow keys, Home, End, Delete), read-only editor, chips, message form with Send and Stop, Accept and Reject with the note that the real app reviews writes automatically, a Commands button for the palette (combobox with a listbox), New chat.
- ejm-harness: sliders for max concurrent and command slots (a committed change is journaled), Add session, Add 5, Crash worker (off while nothing runs), counts, sessions with `progressbar` roles, the journal, and Rust, TypeScript and Python tabs.

Targets are 44px on coarse pointers, inputs are 16px there, and the layout reacts to the demo's own width (container queries), so it works from 320px to 5120px.

### Laziness

`LazyCliDemo` (and `LazyDemo`) serve the static view, which is also what renders without scripts, and loads the island when the demo is within `rootMargin` (default 600px) of the viewport or when the visitor points at, focuses, types in or presses "Try the live demo". The three `import()` calls are separate chunks. `node packages/demos/scripts/size-islands.mjs` (also `npm run size -w @ejm/demos`) builds each island and a tiny page with `<LazyDemo />`, and exits 1 when an island with everything it imports is over 40 KB gzip (React excluded), when island code is in the page's initial chunks, or when the island is not a separate chunk. It also measures the first load each entry adds to an empty React page, and fails when a one-product page carries another product's view or adds more than 14 KB gzip (that figure includes `cn` and `ProductWindow`, which a site ships anyway). Measured: `LazyCliDemo` +11.0 KB, `LazyIdeDemo` +12.6 KB, `LazyHarnessDemo` +12.4 KB, `LazyDemo` +17.1 KB (so one product saves 4.5 to 6 KB); `ProductHeroStage` with `CliHeroWindow` +9.7 KB, `ProductHero` +14.3 KB. Each product has its own frame file (`src/views/frames/<product>.json`) so a one-product page does not ship the other frames. Do not import from `islands/*.tsx` or `parts.tsx` in code that must stay in the initial bundle: `islands/btn.tsx` exists because a static import of `parts.tsx` pulled the whole shell into the entry.

## Product hero

```tsx
<ProductHero eyebrow="ejmlabs / one-person lab" title="Small experiments, useful things." actions={...}>
  <p>Copy.</p>
</ProductHero>
```

```tsx
// only the windows a page needs: one view, one frame
<ProductHeroStage title="Small experiments, useful things." windows={<CliHeroWindow />}>
  <p>Copy.</p>
</ProductHeroStage>
```

`ProductHeroStage` is the layout `Hero` with a `ProductStage` around the windows you give it; `ProductHero` is that with all three, which pulls in all three views. Product imagery is moving to real captures (`ProductShot`), so there are no more simulated-window options than these. `ProductHero` is the layout `Hero` with a `ProductStage` of the cli (rest), harness (raised) and ide (float) views, in that order so the front window is the one with the most to read and the governor numbers sit in the open. The windows are decoration. Entrance, no-JS and reduced-motion behavior are the `Hero`'s: copy first (sliding, never faded, so it is the LCP at first paint), then the windows, 880ms in all, transform and opacity only, everything in place without scripts.

## Honesty rules

1. Every simulated window and interactive island says it is simulated, and an island also states that nothing is sent anywhere. Neither can be hidden or shortened.
2. No claim of real output, no benchmarks, no customers. Every number that is not a count of what is on screen is labelled as a sample value.
3. Every command, flag, key and tool name shown appears in that product's README, or in a file named in `honesty-allow.json` with a reason. `node packages/demos/scripts/check-demos-honesty.mjs` checks this against `~/Desktop/ejm-cli`, `ejm-ide` and `ejm-harness` (or `--root DIR`) and fails on a name that is not documented, on an allow entry whose source does not contain the name, and on a stale entry. Never add an entry to make the gate pass: add it only when the name really is documented elsewhere.
4. The ejm-ide tool identifiers retain the public `workbench_` prefix, including `workbench_search_code`, `workbench_read_file` and `workbench_write_file`. Read the product's current `docs/agent-tools.md`. A simulated palette shortcut is not a product API claim.
5. Where a demo needs a control the product lacks, the control says so: Grant write stands in for `--allow-write`, Accept and Reject exist to show both outcomes.
6. No network, no storage, no cookies and no analytics. `src/islands/no-network.test.ts` fails when a view, island or the hero uses a network, storage or cookie API, and the cli island test spies on `fetch`.

## Recorded CLI entry

`LazyCliShowcase` is a separate named export from `@ejm/demos/islands/cli-lazy`.
It preserves the existing `LazyCliDemo` simulation and takes no editable prompt,
grant control or product command. Use it only with a verified recording. This
caller-owned entry includes no default recording. Bundled native previews below
use a separate additive export.

The product owns its data, narration and capture receipt. Declare one stable
`CliRecordingSource` outside the React component. It supplies `id`,
`initialFrame`, `finalFrame` and `load`. The ID is the 64-character lowercase
SHA-256 of the product's capture input manifest. `load` must only dynamically
import that product's bundled recording. Its `{ id, beats }` result must use the
same ID. Each beat has an `at` timestamp in milliseconds and an `update` with
optional `status` fields (`title`, `agents`, `models`, `tools`) and changed transcript `blocks`. Existing block
IDs are replaced in place; new IDs append.

```tsx
import { LazyCliShowcase, type CliRecordingSource } from '@ejm/demos/islands/cli-lazy';
import frames from './recordings/cli-frames';

// Generate these typed product modules from the reviewed capture. Do not copy sample output.
const source: CliRecordingSource = {
  id: frames.id,
  initialFrame: frames.initialFrame,
  finalFrame: frames.finalFrame,
  load: () => import('./recordings/cli-recording').then(module => module.default),
};

<LazyCliShowcase source={source} />
```

The generated timeline module must satisfy `CliRecording`; its frame module must
satisfy the frame fields in `CliRecordingSource`. This preserves literal block
kinds and roles without treating raw JSON as a checked TypeScript module.

With a stable source, parent rerenders preserve the loaded recording, playback
progress and pause state, including when `options` is a new equivalent object.
Clock and speed options initialize the player; remount it to change them.
Changing the source starts that source's playback from the beginning.

The initial HTML and no-JS view show the compact final frame without importing
the full timeline. On approach, the same lazy loader loads the passive island.
Playback exposes only Pause/Resume and Restart. It suspends offscreen and when
the page is hidden, stops its pending work on unmount, and shows the final frame
without importing the timeline or starting playback timers under reduced motion. The fixed read/write/shell grants
come from the source. They cannot be changed by the visitor.

A source identity mismatch, failed playback bound or rejected loader keeps the
static final frame and shows a generic status message. The identity check catches
stale source/data pairings. It does not authenticate JavaScript served by an
operator or validate the truth of a recording. Review the real input/output
hashes, product source and binary, and generated final frame through the product's
capture and release gates. Label authored narration and timing separately from
observed tool output. Never claim real execution from an unverified payload.

A timeline has 1 to 256 beats, nondecreasing finite timestamps from zero through
120,000 ms, at most 256 distinct transcript blocks and 512 lines per block. CLI
recording JSON is limited to 262,144 UTF-16 code units. These are playback bounds,
not a bundle allowance. The existing 14 KiB gzip initial-load and 40 KiB gzip
island budgets still apply; the size gate also checks the caller's lazy-data
boundary. Real product data needs its own normal site and budget checks.

`LazyHarnessShowcase` has the same source, options, lazy loading, motion and
cleanup contract. Import it and `HarnessRecordingSource` from
`@ejm/demos/islands/harness-lazy`. Its source supplies `HarnessFrame` snapshots;
each `HarnessRecordedUpdate` replaces the supplied fields of a snapshot. Replace
`stats` as a whole, and supply complete replacement `sessions` and `journal`
arrays when those fields change. A beat allows at most 256 sessions and 512
journal entries, within the same 262,144 UTF-16 code-unit serialized-data and
finite-time bounds.
The static `HarnessScreen` keeps its sample-value labels. A capture must state
which displayed values were observed and which labels, progress or samples were
authored. Visitors cannot add sessions, alter capacity or crash a worker here.

The generic adapter in `src/recordings/playback.ts` and the shared
`RecordedIsland` loader reuse the current cancellable runtime and lifecycle.
Neither entry includes a product recording. Each product owns its actual
session counts, observed capacity, transcript, narration and capture receipt.
No recording or execution is claimed by the package's test fixtures. The
simulation honesty rules above still apply to existing interactive entries.

## Bundled native previews

Use the preview entry when a site needs the reviewed Harbor workflow without
owning a new recording. The existing `Lazy*Demo` simulations and source-required
`Lazy*Showcase` APIs keep their contracts.

```tsx
import { ShowcaseFrame } from '@ejm/demos/ShowcaseFrame';
import { LazyCliPreview } from '@ejm/demos/islands/cli-preview';

<ShowcaseFrame label="ejm-cli recorded workflow"
  note="Local recording. No command or model runs in this page.">
  <LazyCliPreview />
</ShowcaseFrame>
```

Import `@ejm/demos/cli.css` after the shared UI stylesheet. For IDE use
`LazyIdePreview` from `@ejm/demos/islands/ide-preview` and
`@ejm/demos/ide-preview.css`. For Harness use `LazyHarnessPreview` from
`@ejm/demos/islands/harness-preview` and `@ejm/demos/harness.css`. The existing
`@ejm/demos/ide.css` remains the interactive simulated-view stylesheet.
These public profiles own their source lists. The native IDE profile also loads the opt-in `@ejm/tokens/neutral.css` scale used by the desktop app. It follows the page theme without loading the full application theme again. Do not add the broad legacy `@ejm/demos/styles.css` unless a page also uses an older simulated view. Do not scan private package paths.

The only preview options are `rootMargin` (default `240px`) and `eager` (default
`false`). The complete static excerpt renders on the server. A one-product
client loads only its own player when it approaches the viewport. Reduced
motion shows the excerpt without scheduling playback. Pause, Resume and Restart
sit outside the passive product surface and use the canonical medium control
recipe. Product inputs are disabled. The native IDE workspace keeps its desktop
geometry in a labelled horizontal scrollport on narrow pages.

Bundled modules validate their data at the existing lazy module boundary and
reuse the shared loaded player, without a second asynchronous data loader.
The source-required `RecordedIsland` keeps its caller-owned loading contract.
The shared player, `createRecordedPlayback` and injected clock own validation,
pause, page/viewport visibility, restart and disposal. The adapters
apply immutable product updates; they add no worker, network client, storage or
second timer engine. Each timeline retains the existing 256-beat, 120-second
and 262,144 UTF-16-code-unit bounds. IDE text updates group two original 20ms
chunks into one update, at most 25 updates per second, without dropping text or
changing the end time. Completed playback returns to the same useful excerpt
as the no-script view. Full workflow data remains in the recording.
The bundled CLI uses the recorded Unicode glyphs and colors. Public interactive
views retain their plain and ASCII modes.

A rejected module or recording keeps the server excerpt and announces that
playback is unavailable. Reload retries the module. The 14 KiB initial-load and
40 KiB island limits still apply, alongside each consuming site's normal gate.

`ShowcaseFrame` supplies one nearby accessible disclosure. Its optional `note`
replaces the default, so project-specific capture facts do not need a second
paragraph. Keep a truthful offline disclosure near each preview. The CLI
contains recorded Cargo/Git results; IDE edits are authored and its tests were
not run; Harness retains historical worker output and admission reservations.
See [preview provenance](src/previews/PROVENANCE.md) for raw artifacts, source
references, hashes, capture commands and limits. These source changes do not
establish a package release or a published site.

## Tests

Engines and scenarios: `src/*.test.ts`. Views, islands and hero: beside their files, with Testing Library, user-event, the manual clock and vitest-axe in both themes. jsdom has no `matchMedia` and cannot move a range input with a key, so `src/test-helpers.tsx` stubs the first and the harness test sends the change event directly.

## Package rights

This package includes its [license](LICENSE) and [third-party notices](THIRD_PARTY_NOTICES.md).
The software uses the MIT terms; supplied brand materials retain their owners'
rights. Dependencies keep their own notices. The package remains private and
can be consumed from an accepted source revision or an offline tarball.
