import type { DocPage } from '@ejm/ui';
import { docMeta } from './meta';

export const components: DocPage = {
  ...docMeta['components'],
  sections: [
    {
      id: 'how-they-work',
      title: 'How components are built',
      blocks: [
        {
          type: 'p',
          children: (
            <>
              Controls are shadcn/ui source copied in and edited here, on React 19. Where a native element replaces a primitive, the component uses the native element: <code>Dialog</code> is the browser's <code>dialog</code>, <code>Select</code> is a native <code>select</code>, and the mobile menus are <code>details</code>. Radix is used where it earns its weight, in menus, tooltips, switches and checkboxes.
            </>
          ),
        },
        {
          type: 'p',
          children: <>Every component has a render test and an accessibility check in light and dark. Components with behavior also have a keyboard test.</>,
        },
      ],
    },
    {
      id: 'controls',
      title: 'Controls',
      blocks: [
        {
          type: 'params',
          caption: 'Controls',
          labels: { name: 'Component', description: 'What it is' },
          rows: [
            { name: 'Button', description: 'Primary, secondary, ghost, link and danger variants. Pass asChild to give a link the same look.' },
            { name: 'Input, Textarea', description: 'Text fields with the control border and the 3:1 focus ring.' },
            { name: 'Select', description: 'A native select with the control look. Phones keep their own picker.' },
            { name: 'Switch, Checkbox', description: 'The button is the 44px touch target; the drawing inside is compact.' },
            { name: 'Badge', description: 'A tag as plain mono text. No fill, no border.' },
            { name: 'Separator', description: 'A hairline. Never a box edge.' },
            { name: 'Tooltip', description: 'A small label for a control. No shadow.' },
            { name: 'Tabs', description: 'Arrows wrap, Home and End jump, and only the current tab is in the tab order.' },
            { name: 'Dialog', description: 'A modal on the native dialog element, with a float shadow.' },
            { name: 'Menu', description: 'A dropdown with typeahead, Escape, and focus back to the trigger.' },
            { name: 'ToastProvider, useToast', description: 'Toasts in a polite live region. F8 moves focus to the stack.' },
          ],
        },
        { type: 'h3', id: 'button', title: 'Button' },
        {
          type: 'params',
          caption: 'Button props',
          rows: [
            { name: 'variant', type: "'primary' | 'secondary' | 'ghost' | 'link' | 'danger'", default: "'primary'", description: 'The look. Primary is the Sage fill with Ink text.' },
            { name: 'size', type: "'sm' | 'md' | 'lg' | 'icon'", default: "'md'", description: 'Heights of 36px, 44px and 56px, or a 44px square for a labelled icon control.' },
            { name: 'density', type: "'comfortable' | 'compact'", default: "'comfortable'", description: 'Compact uses 32px application controls on a fine pointer and keeps 44px targets on touch.' },
            { name: 'arrow', type: 'boolean', default: 'false', description: 'Adds the trailing arrow, which slides 4px on hover and stays put under reduced motion.' },
            { name: 'asChild', type: 'boolean', default: 'false', description: 'Renders the child element, for example a link, with the button look.' },
          ],
        },
        {
          type: 'code',
          lang: 'tsx',
          code: `import { Button } from '@ejm/ui';

<Button asChild arrow>
  <a href="/docs">Read the docs</a>
</Button>`,
        },
      ],
    },
    {
      id: 'layout-parts',
      title: 'Layout parts',
      blocks: [
        {
          type: 'params',
          caption: 'Layout parts',
          labels: { name: 'Component', description: 'What it is' },
          rows: [
            { name: 'Band', description: 'A full-bleed section with a ground: paper, mist, sage or ink.' },
            { name: 'SectionHead', description: 'A kicker, a large balanced heading and supporting copy.' },
            { name: 'Rows, Row', description: 'A hairline list that replaces cards. A row has a title, text, mono meta and an arrow.' },
            { name: 'Facts, Fact', description: 'Mono labels, hairlines and values. Stacks in a narrow column.' },
            { name: 'Stat', description: 'A figure with a mono label. Use size="title" in dense examples. Loading, error and empty are words.' },
            { name: 'Hero', description: 'A type-led opening with a slot for floating product windows. No panel.' },
            { name: 'ProductWindow, ProductStage', description: 'The only layout part with a shadow. Always carries a simulated label unless it is live.' },
            { name: 'Text, Rule, Status, Art', description: 'A run of text, a hairline with a label, a glyph and a word, and decorative line art.' },
          ],
        },
      ],
    },
    {
      id: 'data-and-state',
      title: 'Data and state',
      blocks: [
        {
          type: 'p',
          children: <>These four are shared with the terminal targets, so the web, Ink and Rust versions have the same props and states. The contract in <code>packages/core/contract.json</code> lists them, and a parity script fails when a target drops a state.</>,
        },
        {
          type: 'params',
          caption: 'Contract components',
          labels: { name: 'Component', type: 'States', description: 'What it is' },
          rows: [
            { name: 'Table', type: 'default, empty, loading, error', description: 'Hairline rows, right-aligned numbers, and a scroll region on narrow screens.' },
            { name: 'StateBlock', type: 'empty, loading, error', description: 'One plain sentence and at most one action. Never a bare zero.' },
            { name: 'Prompt', type: 'idle, busy, error', description: 'An input line in the terminal manner.' },
            { name: 'KeyHints', type: 'default', description: 'Key and label pairs in one line of mono text.' },
          ],
        },
        { type: 'callout', kind: 'note', children: <p>Callout has three states, note, tip and warning, and each one is named by a word first. The docs parts below are web only.</p> },
      ],
    },
    {
      id: 'shell',
      title: 'Shell and hooks',
      blocks: [
        {
          type: 'params',
          caption: 'Shell, motion and hooks',
          labels: { name: 'Name', description: 'What it does' },
          rows: [
            { name: 'SiteLayout', description: 'Skip link, Header, main and Footer. The route supplies bands and one h1.' },
            { name: 'Brand', description: 'Canonical artwork with optional light and dark sources, an optional name and shared sizing.' },
            { name: 'DocsLayout', description: 'A sidebar, a 68ch reading column and a contents list, both folding into labelled disclosures on narrow screens.' },
            { name: 'AppShell', description: 'A top bar, a side navigation and content for applications.' },
            { name: 'Header, Footer, ThemeToggle', description: 'The sticky header with a menu below 768px, the Ink footer, and the light and dark switch.' },
            { name: 'ExternalLink, SkipLink, Collapse', description: 'A link to another site with an arrow, the first focusable element, and one native disclosure.' },
            { name: 'Reveal, Stagger, useReveal, useDrawOn', description: 'Entrance and scroll motion. Nothing is hidden without scripts or under reduced motion.' },
            { name: 'useTheme', description: 'Light, dark or system, saved to the ejm-theme cookie.' },
            { name: 'useMediaQuery, useReducedMotion, useContainerQuery, useInView', description: 'Server-safe measurements. The first render matches the server.' },
            { name: 'useClipboard, useHotkeys', description: 'Copy with a confirmation, and keyboard shortcuts that ignore typing in fields.' },
          ],
        },
      ],
    },
    {
      id: 'docs-parts',
      title: 'Docs parts',
      blocks: [
        {
          type: 'p',
          children: <>This page is built from them. They live in <code>@ejm/ui</code>, and a page is plain data: a title, a lead and sections.</>,
        },
        {
          type: 'params',
          caption: 'Docs parts',
          labels: { name: 'Part', description: 'Behavior' },
          rows: [
            { name: 'DocsSidebar', description: 'Grouped page links with aria-current. A filter narrows them and says how many results.' },
            { name: 'Contents', description: 'On this page. Links work without scripts. With scripts it marks the heading being read and moves focus to a heading you choose.' },
            { name: 'CodeBlock', description: 'A language label, a Copy button that says Copied, and a code area that scrolls inside itself.' },
            { name: 'CodeTabs', description: 'The tabs pattern for the same task in several languages. With no script every panel shows under its label.' },
            { name: 'Callout', description: 'Note, Tip or Warning: a left rule and a word first.' },
            { name: 'ParamTable', description: 'Names in mono, tabular figures, and a scroll region on narrow screens.' },
            { name: 'Pager', description: 'Previous and next links that name the pages.' },
          ],
        },
        {
          type: 'tabs',
          label: 'Language',
          tabs: [
            {
              label: 'TypeScript',
              lang: 'tsx',
              code: `import { Callout, CodeBlock } from '@ejm/ui';

<CodeBlock lang="css" code='@import "@ejm/ui/styles.css";' />
<Callout kind="tip">Preview both themes side by side.</Callout>`,
            },
            { label: 'Data', lang: 'ts', code: "const page: DocPage = {\n  slug: 'theming',\n  group: 'Foundations',\n  title: 'Theming and the cookie',\n  lead: 'Light and dark follow the visitor.',\n  sections: [{ id: 'cookie', title: 'The cookie', blocks: [{ type: 'p', children: 'One cookie.' }] }],\n};" },
          ],
        },
      ],
    },
  ],
};
