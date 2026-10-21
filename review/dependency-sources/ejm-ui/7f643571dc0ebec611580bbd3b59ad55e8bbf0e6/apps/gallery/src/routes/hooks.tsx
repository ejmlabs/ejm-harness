import { Button, Input, Reveal, Stagger, useClipboard, useContainerQuery, useDrawOn, useHotkeys, useInView, useMediaQuery, useReducedMotion, useTheme } from '@ejm/ui';
import { useId, useState, type ReactNode } from 'react';

// Live demos of the hooks and motion primitives. Each section is one hook, its value, and the control that changes it.

function Demo({ name, children, lead }: { name: string; lead: string; children: ReactNode }) {
  const headingId = useId();
  return (
    <Reveal as="section" className="hairline-t grid gap-x-12 gap-y-4 py-10 lg:grid-cols-12" aria-labelledby={headingId}>
      <div className="lg:col-span-6">
        <h2 id={headingId} className="text-heading font-mono">{name}</h2>
        <p className="mt-3 max-w-measure text-muted">{lead}</p>
      </div>
      <div className="lg:col-span-6">{children}</div>
    </Reveal>
  );
}

function ThemeDemo() {
  const { choice, resolved, setChoice, toggle } = useTheme();
  return (
    <>
      <p className="m-0 text-meta">choice: {choice} / on screen: {resolved}</p>
      <p className="mt-4 flex flex-wrap gap-3">
        {(['light', 'dark', 'system'] as const).map(c => (
          <Button key={c} type="button" variant={choice === c ? 'secondary' : 'ghost'} aria-pressed={choice === c} onClick={() => setChoice(c)}>{c}</Button>
        ))}
        <Button type="button" variant="secondary" onClick={toggle}>toggle</Button>
      </p>
    </>
  );
}

function MediaDemo() {
  const wide = useMediaQuery('(min-width: 768px)');
  const reduced = useReducedMotion();
  return (
    <p className="m-0 text-meta">
      min-width 768px: {String(wide)}
      <br />
      prefers-reduced-motion: {String(reduced)}
    </p>
  );
}

function ContainerDemo() {
  const { ref, width, matches } = useContainerQuery<HTMLDivElement>({ minWidth: 420 });
  return (
    <>
      <p className="m-0 text-meta">Drag the corner of the line below. Width {width ?? 'unmeasured'}px, at least 420px: {String(matches)}</p>
      <div ref={ref} className="mt-4 h-14 min-w-48 w-full border-b border-control" style={{ resize: 'horizontal', overflow: 'auto' }}>
        <p className="m-0 pt-1 text-meta text-muted">{matches ? 'wide layout' : 'narrow layout'}</p>
      </div>
    </>
  );
}

function InViewDemo() {
  const { ref, inView } = useInView<HTMLParagraphElement>();
  return (
    <p ref={ref} className="m-0 text-meta">
      This line is {inView ? 'in view' : 'out of view'}.
    </p>
  );
}

function ClipboardDemo() {
  const { label, announcement, copy } = useClipboard();
  return (
    <>
      <p className="m-0 flex flex-wrap items-center gap-4">
        <code className="text-meta">npm i @ejm/ui</code>
        <Button type="button" variant="secondary" onClick={() => void copy('npm i @ejm/ui')}>{label}</Button>
      </p>
      <p role="status" className="mt-3 text-meta text-muted">{announcement}</p>
    </>
  );
}

function HotkeysDemo() {
  const [log, setLog] = useState<string[]>([]);
  const push = (k: string) => setLog(l => [k, ...l].slice(0, 4));
  useHotkeys({ '?': () => push('?'), '/': () => push('/'), 'mod+k': () => push('mod+k'), g: () => push('g') });
  return (
    <>
      <p className="m-0 text-meta">Press ?, /, g or mod+k (not while typing in the field).</p>
      <p className="mt-4"><Input aria-label="Typing here does not fire plain keys" placeholder="Type here" /></p>
      <p className="mt-2 text-meta text-muted">{log.length ? `last: ${log.join(', ')}` : 'no keys yet'}</p>
    </>
  );
}

function Art() {
  const ref = useDrawOn<SVGSVGElement>();
  return (
    <svg ref={ref} viewBox="0 0 120 60" className="art max-w-64" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2">
      <path pathLength="1" d="M4 50 C 30 4, 60 4, 116 50" />
      <path pathLength="1" d="M4 30 H 116" />
      <path pathLength="1" d="M60 4 V 56" />
    </svg>
  );
}

export default function HooksPage() {
  return (
    <>
      <div className="page py-rhythm-sm">
        <p className="text-meta text-muted">Hooks and motion</p>
        <h1 className="mt-4 max-w-lg text-display">Behavior, not boxes.</h1>
        <p className="mt-6 max-w-measure text-lead text-muted">Every hook is safe to render on the server. Scroll down: each section reveals once as it arrives. Turn on reduced motion in the system and everything is simply there.</p>

        <div className="mt-16">
          <Demo name="useTheme" lead="Light, dark or system. Saved to the ejm-theme cookie on ejmlabs.com, localStorage on other hosts."><ThemeDemo /></Demo>
          <Demo name="useMediaQuery, useReducedMotion" lead="A media query as state. Server and hydration render false, then the real value arrives."><MediaDemo /></Demo>
          <Demo name="useContainerQuery" lead="Adapt to the space a component has, measured with ResizeObserver."><ContainerDemo /></Demo>
          <Demo name="useInView" lead="True while the element is on screen."><InViewDemo /></Demo>
          <Demo name="useClipboard" lead="Copy with a confirmation in words that returns to idle."><ClipboardDemo /></Demo>
          <Demo name="useHotkeys" lead="Scoped shortcuts that ignore typing and clean up after themselves."><HotkeysDemo /></Demo>
          <Demo name="Reveal" lead="Plays once when it scrolls into view. The five lines below arrive with index 0 to 4 and four directions.">
            <div className="grid gap-3">
              {(['up', 'fade', 'scale', 'left'] as const).map((from, i) => (
                <Reveal key={from} from={from} index={i} className="hairline pb-2">from {from}, index {i}</Reveal>
              ))}
            </div>
          </Demo>
          <Demo name="Stagger" lead="A staged entrance for what is on screen at load. The index stops at 8, so it ends in under 1.2 seconds.">
            <Stagger as="ul" className="m-0 list-none p-0">
              {Array.from({ length: 10 }, (_, i) => <li key={i} className="hairline-t py-2 text-meta">step {i}</li>)}
            </Stagger>
          </Demo>
          <Demo name="useDrawOn" lead="Draws the art on once when it is seen. Without scripts or under reduced motion it is fully drawn."><Art /></Demo>
        </div>
      </div>
    </>
  );
}
