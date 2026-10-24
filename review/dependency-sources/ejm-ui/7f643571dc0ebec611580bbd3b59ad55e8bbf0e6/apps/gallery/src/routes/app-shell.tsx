import { Link } from '@ejm/ssg';
import logoInk from '@ejm/tokens/assets/projects/ide/logo-ink.svg?url';
import logoPaper from '@ejm/tokens/assets/projects/ide/logo-paper.svg?url';
import { AppShell, Brand, Button } from '@ejm/ui';

// AppShell is a whole page frame (top bar, side nav, content), so it has its own route.

const items = [
  ['Files', '#files'],
  ['Search', '#search'],
  ['Agent', '#agent'],
  ['Settings', '#settings'],
] as const;

export default function AppShellPage() {
  return (
    <AppShell
      linkComponent={Link}
      brand={<Brand src={logoInk} darkSrc={logoPaper} width={605} height={256} />}
      homeLabel="ejm-ide home"
      nav={
        <ul className="m-0 list-none p-0">
          {items.map(([label, href], i) => (
            <li key={label}>
              <a href={href} aria-current={i === 0 ? 'page' : undefined} className="touch-target flex items-center py-2 text-fg hover:text-link aria-[current=page]:font-semibold aria-[current=page]:underline aria-[current=page]:decoration-2">
                {label}
              </a>
            </li>
          ))}
        </ul>
      }
      actions={
        <Button asChild variant="ghost" size="sm"><Link href="/shell/">
          Shell
        </Link></Button>
      }
    >
      <h1 className="text-title">Workspace</h1>
      <p className="mt-4 max-w-measure text-muted">
        The top bar and the side nav stay while the page scrolls. The nav folds into a labelled disclosure below 768px. Nothing here is a box: the nav is separated by one hairline.
      </p>
      <ul className="hairline-t mt-10 max-w-measure">
        {Array.from({ length: 12 }, (_, i) => (
          <li key={i} className="hairline flex items-baseline justify-between gap-x-6 py-4">
            <span>src/module-{i + 1}.rs</span>
            <span className="text-meta tabular-nums text-muted">{(i + 1) * 3} KB</span>
          </li>
        ))}
      </ul>
    </AppShell>
  );
}
