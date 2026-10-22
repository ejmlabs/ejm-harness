import type { DocPage } from '@ejm/ui';
import { docMeta } from './meta';

export const gettingStarted: DocPage = {
  ...docMeta['getting-started'],
  sections: [
    {
      id: 'what-it-is',
      title: 'What ejm-ui is',
      blocks: [
        {
          type: 'p',
          children: (
            <>
              ejm-ui holds the tokens, the Tailwind theme, React components in the shadcn/ui style, hooks, layouts and the gallery you are reading. It is one npm workspace with real React 19, strict TypeScript and
              Tailwind CSS v4.
            </>
          ),
        },
        {
          type: 'params',
          caption: 'Packages in the workspace',
          labels: { name: 'Package', description: 'What it holds' },
          rows: [
            { name: '@ejm/tokens', description: 'theme.json, brand assets and the generators for CSS, Tailwind, Rust, Monaco and terminal themes.' },
            { name: '@ejm/ui', description: 'Components, hooks, layouts, docs parts, styles and art for the web.' },
            { name: '@ejm/core', description: 'Framework-free logic: state machines, the component contract and formatting. It has no React in it.' },
            { name: '@ejm/demos', description: 'Scenario data and the demo engines for the product pages.' },
            { name: '@ejm/ssg', description: 'The prerender package for Vite and React sites: static HTML for every route, hydration, links and the budget gate.' },
            { name: '@ejm/site', description: 'What every ejmlabs site shares: the project registry, analytics, the shared head and the security headers.' },
            { name: '@ejm/gallery', description: 'This site: the gallery and the docs, a prerendered Vite and React site built on @ejm/ssg and @ejm/site.' },
          ],
        },
        {
          type: 'callout',
          kind: 'note',
          children: <p>ejm-ui is local only. It has no remote and no published packages yet, so you use it from this workspace.</p>,
        },
      ],
    },
    {
      id: 'install',
      title: 'Install and run',
      blocks: [
        { type: 'p', children: <>You need Node 22 or later. The repo uses npm workspaces, so one install covers every package.</> },
        {
          type: 'code',
          lang: 'sh',
          code: `npm install
npm run dev        # the gallery dev server (ejm-ssg dev)
npm run build      # build every package and app; the gallery prerenders every page
npm run check      # text checks, type check and tests`,
        },
        { type: 'p', children: <>Run <code>npm run check</code> before you commit. <a href="/docs/checks">Checks and budgets</a> lists what it protects.</> },
      ],
    },
    {
      id: 'first-page',
      title: 'Your first page',
      blocks: [
        { type: 'h3', id: 'stylesheet', title: 'Import the stylesheet' },
        {
          type: 'p',
          children: (
            <>
              A site has one entry stylesheet. It loads Tailwind, the generated color theme, the two brand fonts, the type scale, the page column and the motion rules.
            </>
          ),
        },
        { type: 'code', lang: 'css', code: '@import "@ejm/ui/styles-site.css" source(none);' },
        { type: 'p', children: <>The site profile includes shared layouts and Button. Disabling the automatic scan keeps unrelated installed package styles out of the bundle. Use the complete <code>styles.css</code> profile for form and application controls. Add <code>@ejm/ui/styles-docs.css</code> for documentation. Native product previews import the public <code>@ejm/demos/cli.css</code>, <code>ide-preview.css</code> or <code>harness.css</code> profile. The existing <code>ide.css</code> entry still styles the interactive simulated IDE. These imports own their source lists; do not scan private package paths.</> },
        { type: 'p', children: <>Use the public Button and Input components for controls, Brand for approved artwork, and DocsSidebar and Contents for documentation navigation. Compose site content with SectionBody, Columns, ContentFlow and Actions. Add missing visual patterns to ejm-ui; public sites do not author CSS or utility classes.</> },
        { type: 'p', children: <>The gallery links a complete stylesheet per route through <code>@ejm/ssg</code> head links, so docs, app examples and recorded demos load their own required styles before hydration. Interactive examples stay disabled while page code loads and enable when their handlers are ready. With scripts off, their content remains readable.</> },
        { type: 'p', children: <>If a page chunk cannot load, a recovery message offers <strong>Reload this page</strong>. Reconnect first if you are offline. Reloading requests the latest HTML and can recover a tab opened before a release.</> },
        {
          type: 'p',
          children: (
            <>
              In the head, set the color scheme, load <code>theme-boot.js</code> before any CSS paints, and preload the two fonts. <a href="/docs/theming">Theming and the cookie</a> explains the script.
            </>
          ),
        },
        {
          type: 'code',
          lang: 'html',
          code: `<head>
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <meta name="color-scheme" content="light dark" />
  <script src="/theme-boot.js"></script>
</head>`,
        },
        { type: 'h3', id: 'compose', title: 'Build with bands' },
        {
          type: 'p',
          children: (
            <>
              A page is a stack of full-bleed bands. Each band sets its own ground, and everything inside reads correctly on it. Rows, type and space do the grouping, so there are no cards.
            </>
          ),
        },
        {
          type: 'code',
          lang: 'tsx',
          code: `import { Band, Row, Rows, SectionBody, SectionHead } from '@ejm/ui';

export default function Projects() {
  return (
    <Band tone="paper" aria-labelledby="projects-title">
      <SectionHead kicker="01 / Projects" title="Three tools, one engine." id="projects-title">
        <p>ejm-harness is the engine. ejm-cli and ejm-ide run on it.</p>
      </SectionHead>
      <SectionBody><Rows aria-label="Projects">
        <Row title="ejm-cli" href="/cli" description="A Rust terminal coding agent." meta="Rust / terminal" />
      </Rows></SectionBody>
    </Band>
  );
}`,
        },
        { type: 'h3', id: 'read-a-token', title: 'Read a token anywhere' },
        {
          type: 'p',
          children: (
            <>The same color has a name in every layer. The primary fill is Sage on a light page. Use the semantic name, never a hex value.</>
          ),
        },
        {
          type: 'tabs',
          label: 'Language',
          tabs: [
            { label: 'CSS', lang: 'css', code: '.button {\n  background: var(--ejm-primary);\n  color: var(--ejm-on-primary);\n}' },
            { label: 'TypeScript', lang: 'tsx', code: '<a href="/start" className="bg-primary text-on-primary hover:bg-primary-hover">\n  Start\n</a>' },
            { label: 'Rust', lang: 'rust', code: 'use ejm_theme::DARK;\n\nlet fill = DARK.primary;' },
          ],
        },
      ],
    },
    {
      id: 'next',
      title: 'Where to go next',
      blocks: [
        {
          type: 'list',
          items: [
            <><a href="/docs/design-language">Design language</a> for the rules that keep every page one family.</>,
            <><a href="/docs/tokens-and-bands">Tokens and bands</a> for every color name and what it is for.</>,
            <><a href="/docs/components">Components</a> for what exists before you build something new.</>,
          ],
        },
      ],
    },
  ],
};
