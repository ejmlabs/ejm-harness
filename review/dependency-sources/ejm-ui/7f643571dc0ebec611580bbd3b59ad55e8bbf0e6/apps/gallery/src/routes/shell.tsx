import { Link } from '@ejm/ssg';
import symbolInk from '@ejm/tokens/assets/logos/symbol-ink.svg?url';
import symbolPaper from '@ejm/tokens/assets/logos/symbol-paper.svg?url';
import { Contents, DocsLayout, DocsSidebar, ExternalLink, Header, Reveal, type HeaderProps } from '@ejm/ui';

// Header, footer and layouts, shown the way a site uses them. The gallery's SiteLayout (src/frame.tsx) wraps this whole page. The previews
// below use a data-theme on a wrapper to show both themes side by side; the toggle in each still switches the real page.

const nav: HeaderProps['nav'] = [
  { label: 'Docs', href: '/docs/' },
  { label: 'Components', href: '/components/' },
  { label: 'Layout', href: '/layout/' },
  { label: 'App', href: '/app/' },
  { label: 'Product', href: '/product/' },
];
const header: Omit<HeaderProps, 'identity'> = { homeLabel: 'ejm-ui home', nav };

function Preview({ theme }: { theme: 'light' | 'dark' }) {
  return (
    <section data-theme={theme} aria-label={`Header preview, ${theme} theme`} className="band relative">
      <p className="page-inset pt-6 text-meta text-muted">{theme} theme</p>
      <Header {...header} identity={{ name: 'ejm-ui', artwork: { src: symbolInk, darkSrc: symbolPaper, width: 418, height: 381 } }} />
    </section>
  );
}

const sidebar = <DocsSidebar current="/shell/" filter={false} groups={[{
  title: 'Gallery',
  items: [
    { label: 'Get started', href: '/docs/' },
    { label: 'Components', href: '/components/' },
    { label: 'Hooks', href: '/hooks/' },
    { label: 'Layouts', href: '/layout/' },
    { label: 'Shell', href: '/shell/' },
    { label: 'Theme', href: '/docs/theming/' },
  ],
}]} />;
const contents = <Contents items={[{ id: 'docs-article', label: 'Reading column', level: 3 }]} />;

export default function ShellPage() {
  return (
    <>
      <section className="page py-rhythm">
        <p className="text-meta text-muted">Shell</p>
        <h1 className="mt-4 max-w-lg text-display">Header, footer and layouts.</h1>
        <p className="mt-6 max-w-measure text-lead text-muted">
          Every page of this gallery sits in a SiteLayout: skip link, sticky header with the theme toggle last, <code className="text-meta">main#main</code> and the footer below. Narrow the window to
          fold the nav into the menu. The frame for applications is on its own page, <Link href="/app-shell/" className="link">App shell</Link>. Read more about ejmlabs on <ExternalLink href="https://ejmlabs.com">ejmlabs.com</ExternalLink>.
        </p>
      </section>

      <section data-band="mist" className="band py-rhythm-sm" aria-labelledby="previews">
        <div className="page">
          <Reveal as="h2" id="previews" className="text-title">Header in both themes</Reveal>
          <p className="mt-4 max-w-measure text-muted">The toggle is always the last item. On a phone the nav is one Menu button that opens an overlay, with no layout shift.</p>
        </div>
        <div className="mt-10 grid gap-10">
          <Preview theme="light" />
          <Preview theme="dark" />
        </div>
      </section>

      <section className="py-rhythm-sm" aria-labelledby="docs-layout">
        <div className="page">
          <Reveal as="h2" id="docs-layout" className="text-title">DocsLayout</Reveal>
          <p className="mt-4 max-w-measure text-muted">Sidebar and contents are native details below 1024px and 1280px, shown as columns above. The article keeps a 68ch column.</p>
        </div>
        <DocsLayout sidebar={sidebar} contents={contents}>
          <article>
            <h3 id="docs-article" className="text-heading">An article in the reading column</h3>
            <p className="mt-4">
              Lines stop at 68 characters, so the eye can find the next line. The docs parts fill the two slots; this layout only owns the grid, the collapsing and the width.
            </p>
            <p className="mt-4">
              With scripts off, the disclosures still open and close, and nothing is hidden. Resize the window to see the sidebar and the contents join the page as columns.
            </p>
          </article>
        </DocsLayout>
      </section>
    </>
  );
}
