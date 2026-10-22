import type { CSSProperties } from 'react';
import { BANDS, HUES, type Band as BandName } from '@ejm/tokens';
import { Band, Button, SectionHead } from '@ejm/ui';

// The contract page: every band, the type scale, elevation and motion, drawn only with the utilities @ejm/ui/styles.css defines.
// It is also the screenshot target for the band, theme and size checks.

const names: Record<BandName, string> = { paper: 'Paper', mist: 'Mist', sage: 'Sage', ink: 'Ink' };
const shadowClass = { rest: 'shadow-rest', raised: 'shadow-raised', float: 'shadow-float' } as const;
const rows = [
  ['ejm-cli', 'Terminal agent', '0.4'],
  ['ejm-ide', 'Editor and engine', '0.2'],
  ['ejm-harness', 'Test harness', '0.1'],
] as const;

function BandSample({ band }: { band: BandName }) {
  return (
    <section data-band={band} className="band py-rhythm">
      <div className="page">
        <p className="text-meta text-muted">{band} band</p>
        <h2 className="mt-4 max-w-lg text-display-lg">{names[band]} sets the ground.</h2>
        <p className="mt-6 max-w-measure text-lead text-muted">
          Structure comes from display type, space and hairline rows. Muted text, links and rules all keep their contrast on this band.
        </p>
        <ul className="hairline-t mt-12 max-w-measure">
          {rows.map(([name, what, version], i) => (
            <li key={name} className="hairline flex items-baseline justify-between gap-x-6 py-5 animate-enter" style={{ '--ejm-i': i } as CSSProperties}>
              <span className="text-heading">{name}</span>
              <span className="text-meta tabular-nums text-muted">v{version}</span>
              <span className="text-muted">{what}</span>
            </li>
          ))}
        </ul>
        <p className="mt-10 flex flex-wrap items-center gap-x-8 gap-y-4">
          <Button asChild><a href="#main">
            Primary action
          </a></Button>
          <Button asChild variant="link"><a href="#main">A plain link</a></Button>
          <span className="text-link-large text-heading">Large accent</span>
        </p>
        <p className="mt-8 flex flex-wrap gap-x-6 gap-y-2 text-meta">
          <span className="text-success">success</span>
          <span className="text-warn">warn</span>
          <span className="text-danger">danger</span>
          <span className="text-info">info</span>
        </p>
        {band !== 'sage' && (
          <p className="mt-6 flex items-center gap-3" aria-label="Six hue roles">
            {HUES.map(h => (
              <span key={h} className="size-6 rounded-pill" style={{ backgroundColor: `var(--ejm-${h})` }} />
            ))}
          </p>
        )}
      </div>
    </section>
  );
}

export function Tokens() {
  return (
    <>
      <Band tone="paper" aria-labelledby="tokens-title">
        <SectionHead purpose="page" kicker="ejm-ui / tokens" title="Four bands, one type scale." as="h1" id="tokens-title">
          <p className="m-0">Every band, the type scale, elevation and motion, drawn with only the utilities that the theme defines.</p>
        </SectionHead>
      </Band>
      {BANDS.map(b => <BandSample key={b} band={b} />)}
      <section className="band py-rhythm">
        <div className="page">
          <p className="text-meta text-muted">type</p>
          <div className="mt-8">
            <p className="hairline-t py-4 text-display-lg">Display large</p>
            <p className="hairline-t py-4 text-display">Display</p>
            <p className="hairline-t py-4 text-title">Title</p>
            <p className="hairline-t py-4 text-heading">Heading</p>
            <p className="hairline-t py-4 text-lead">Lead paragraph text</p>
            <p className="hairline-t py-4 text-body">Body text at 16 to 20 pixels.</p>
            <p className="hairline-t py-4 text-meta">META 0123456789 mono</p>
            <p className="hairline-t py-4 text-stat tabular-nums">1,284</p>
          </div>
        </div>
      </section>
      <section data-band="mist" className="band py-rhythm">
        <div className="page flex flex-wrap gap-10">
          {(['rest', 'raised', 'float'] as const).map(level => (
            <div key={level} className={`scheme-page w-full max-w-64 rounded-window bg-popover p-6 text-ui-text ${shadowClass[level]}`}>
              <p className="text-meta">shadow-{level}</p>
            </div>
          ))}
        </div>
      </section>
    </>
  );
}
