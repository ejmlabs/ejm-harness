import type { DocPage } from '@ejm/ui';
import { docMeta } from './meta';

export const checks: DocPage = {
  ...docMeta['checks'],
  sections: [
    {
      id: 'bar',
      title: 'The quality bar',
      blocks: [
        {
          type: 'p',
          children: <><code>QUALITY.md</code> covers content, visual design, access and performance. Use the fewest words that preserve meaning. Prefer a clear visual or omit redundant copy. Keep labels, accessible names, consent and security facts, honest state and recovery.</>,
        },
        {
          type: 'list',
          ordered: true,
          items: [
            <>Build the page from <code>@ejm/ui</code> components, layouts and views.</>,
            <>Run <code>npm run check</code>, <code>npm run build</code> and the budget script.</>,
            <>Look at the page in light and dark with the matrix script.</>,
            <>Walk the quality criteria. Record any exception.</>,
          ],
        },
      ],
    },
    {
      id: 'gates',
      title: 'Gates',
      blocks: [
        {
          type: 'params',
          caption: 'Quality gates',
          labels: { name: 'Gate', type: 'Command', description: 'Fails on' },
          rows: [
            { name: 'Text, types and tests', type: 'npm run check', description: 'Em dashes, stale generated files, changed brand assets, type errors, failing tests and the application layout browser gate after the gallery build.' },
            { name: 'Site design ownership', type: 'ejm-ssg build', description: 'Consumer CSS rules, visual classes, inline styles and unsupported shared overrides. The @ejm/site Vite check is mandatory, including unused source files.' },
            { name: 'Contrast', type: 'node packages/tokens/scripts/contrast.mjs --verbose', description: 'Text under 4.5:1, control borders or focus under 3:1, in both themes and on every band.' },
            { name: 'Viewport matrix', type: 'node scripts/shoot.mjs <url>', description: 'Horizontal overflow from 320px to 5120px. It also reports touch targets under 44px.' },
            { name: 'Application layout', type: 'npm run check:layout', description: 'Missing browser evidence, wrong spacing or heading alignment, unequal table loading/data padding, overflow or small touch targets at 360px and 1280px in light and dark.' },
            { name: 'Budgets', type: 'node scripts/size.mjs [dist]', description: 'HTML, CSS, JavaScript or font size over its budget.' },
            { name: 'Parity', type: 'node scripts/parity.mjs', description: 'A web, Ink or Rust target that lacks a component or a state from the contract.' },
          ],
        },
        { type: 'h3', id: 'matrix', title: 'The device matrix' },
        {
          type: 'p',
          children: <>The matrix script starts headless Chrome and drives it over the DevTools Protocol, with no dependencies. It shoots one URL at 14 sizes, from an iPhone SE at 320px to a 5120px super ultrawide. It exits non-zero if any size overflows. Run it in both themes.</>,
        },
        {
          type: 'tabs',
          label: 'Theme',
          tabs: [
            { label: 'Light', lang: 'sh', code: 'node scripts/shoot.mjs http://localhost:4874/docs --theme light --out /tmp/docs' },
            { label: 'Dark', lang: 'sh', code: 'node scripts/shoot.mjs http://localhost:4874/docs --theme dark --out /tmp/docs' },
          ],
        },
        { type: 'callout', kind: 'tip', children: <p>Use <code>--only iphone-mini,laptop,ultrawide-45</code> for a quick look, and <code>--full</code> to save the whole page.</p> },
        { type: 'h3', id: 'application-layout', title: 'Check a complete application page' },
        {
          type: 'p',
          children: <>The <a href="/application-layout/">application layout example</a> groups a title and description, form fields, a prose link, a standalone action and every table state in both densities. The gate measures rendered padding, alignment and gaps, then saves full-page screenshots and JSON reports. It fails when evidence is missing. Product pages still need checks for their own content and conditional states.</>,
        },
        {
          type: 'p',
          children: <>Install Google Chrome or Chromium and use Node 22 or later. Set <code>CHROME=/path/to/chrome</code> for a nonstandard installation. Missing Chrome is a failure, not a skipped check. The normal <code>npm run check</code> command reuses the gallery build from its test suite. To run the gate alone, build the gallery first:</>,
        },
        { type: 'code', lang: 'sh', code: 'npm run build -w @ejm/gallery\nnpm run check:layout -- --out /tmp/ejm-application-layout-review' },
        {
          type: 'p',
          children: <>The output holds light/dark measurement reports, full-page PNG captures and <code>summary.json</code>. Use <code>--dist &lt;dir&gt;</code> to check another built gallery. Without <code>--out</code>, evidence goes under <code>ejm-application-layout</code> in the system temporary directory.</>,
        },
      ],
    },
    {
      id: 'budgets',
      title: 'Budgets',
      blocks: [
        {
          type: 'p',
          children: <>Each prerendered route is measured on what it loads first. Sizes are gzip, except fonts, which are WOFF2. A breach exits non-zero and fails the build.</>,
        },
        {
          type: 'params',
          caption: 'Performance budgets per route',
          labels: { name: 'Measure', type: 'Budget', description: 'Notes' },
          rows: [
            { name: 'JavaScript', type: '95 KB', description: 'React and react-dom (about 71 KB, measured), the router (wouter), hydration, theme and header. A page whose own code does not fit hydrates after the first scripts, with its HTML already complete. The page body starts loading immediately after the site entry, with controls disabled until ready. The byte gate excludes later dynamic chunks; it does not measure the full session download.' },
            { name: 'CSS', type: '14 KB', description: 'Complete stylesheets are linked per route in the server HTML. The gate includes the native IDE styles on the Demos page.' },
            { name: 'Fonts', type: '70 KB', description: 'The two preloaded Latin subsets, with font-display swap and metric-matched fallbacks.' },
            { name: 'HTML', type: '30 KB', description: 'Per route, and every route reads correctly with scripts off.' },
            { name: 'Largest Contentful Paint', type: '1.2 s', description: 'Target on a throttled mobile profile. The byte gate does not measure paint time.' },
            { name: 'Cumulative Layout Shift', type: '0.05', description: 'Target for layout stability. The synchronous theme script reduces theme flashes; a browser measurement must verify the score.' },
          ],
        },
        { type: 'callout', kind: 'warning', children: <p>Change a budget in the design document, not in the script alone. The numbers are a starting point and are tuned from measurement.</p> },
      ],
    },
    {
      id: 'accessibility',
      title: 'Accessibility',
      blocks: [
        {
          type: 'list',
          items: [
            <>Text is at least 4.5:1, and control borders and focus rings at least 3:1, in light and dark.</>,
            <>Meaning never relies on color alone. Callouts, status and chart series carry text labels.</>,
            <>Every page has a skip link, one h1, no skipped heading levels and a full keyboard path.</>,
            <>Components with behavior follow the WAI-ARIA pattern and announce their result.</>,
          ],
        },
      ],
    },
  ],
};
