import ripple from '@ejm/ui/art/inline/divider-ripple.svg?raw';
import { Band, Button, Fact, Facts, Hero, ProductStage, Row, Rows, Rule, SectionHead, Stat, Status, Text } from '@ejm/ui';
import type { RowState, StatState } from '@ejm/core';
import { Cli, Harness, Ide } from '../windows';

// The layout showcase: the containerless language end to end, built only from layout components and the theme contract.
// Words come from the ejmlabs project READMEs; the product images are real captures with provenance captions.

const GITHUB = 'https://github.com/ejmlabs';

const rowStates: { title: string; state: RowState; description: string }[] = [
  { title: 'default', state: 'default', description: 'A row with no marker.' },
  { title: 'selected', state: 'selected', description: 'Marked by a glyph and a held tone.' },
  { title: 'focused', state: 'focused', description: 'Keyboard focus, marked by a glyph and a ring.' },
  { title: 'disabled', state: 'disabled', description: 'No link, and the word says why.' },
];

const statStates: StatState[] = ['default', 'loading', 'error', 'empty'];

export default function Layout() {
  return (
    <>
      <Hero
        art={ripple}
        drawOn
        eyebrow="ejmlabs / open source"
        title="Open source. Room to explore."
        aside={
          // ejm-shots-allow: layout demonstration of ProductStage with real ProductShot children.
          <ProductStage>
            <Cli />
            <Ide />
            <Harness />
          </ProductStage>
        }
        actions={
          <>
            <Button asChild arrow><a href="#projects">See the projects</a></Button>
            <Button asChild variant="link"><a href={GITHUB} rel="external">Read the source</a></Button>
          </>
        }
      >
        <p className="m-0">ejmlabs is a home for open source projects. ejm-cli, ejm-ide and ejm-harness are released under the MIT license. Read the source, report an issue or help shape what comes next.</p>
      </Hero>

      <Band id="projects" tone="sage" aria-labelledby="projects-title">
        <SectionHead kicker="01 / Projects" title="Three tools, one engine." id="projects-title">
          <p className="m-0">ejm-harness is the engine. ejm-cli and ejm-ide are separate hosts that run on it, and all three are open source.</p>
        </SectionHead>
        <Rows size="title" className="mt-16" aria-label="Projects">
          <Row title="ejm-cli" href={`${GITHUB}/ejm-cli`} external description="A Rust terminal coding agent built on ejm-harness. Offline replay needs no account or server." meta="Rust / terminal" />
          <Row title="ejm-ide" href={`${GITHUB}/ejm-ide`} external description="An open-source desktop IDE for writing and understanding code, with an agent beside the editor." meta="Tauri / macOS" />
          <Row title="ejm-harness" href={`${GITHUB}/ejm-harness`} external description="A reusable Rust engine for coding agents, with one resource governor and a durable event journal." meta="Rust / engine" />
        </Rows>
      </Band>

      <Band tone="mist" aria-labelledby="states-title">
        <SectionHead kicker="02 / States" title="Every state has a word." id="states-title">
          <p className="m-0">Status, rows and figures name their state in words and glyphs, never by color alone.</p>
        </SectionHead>
        <div className="mt-16 grid gap-x-12 gap-y-14 lg:grid-cols-12">
          <div className="lg:col-span-6">
            <Rule label="Status" />
            <ul className="m-0 mt-6 flex list-none flex-col gap-4 p-0">
              {(['ok', 'warn', 'error', 'info', 'pending'] as const).map(s => (
                <li key={s}><Status status={s} className="text-body" /></li>
              ))}
            </ul>
            <Rule label="Text" className="mt-12" />
            <p className="mt-6 mb-0">
              <Text>default</Text>, <Text variant="muted">muted</Text>, <Text variant="strong">strong</Text>, <Text variant="code">code</Text>
            </p>
          </div>
          <div className="lg:col-span-6">
            <Rule label="Row" />
            <Rows className="mt-6" aria-label="Row states">
              {rowStates.map(r => (
                <Row key={r.state} title={r.title} state={r.state} description={r.description} href="#main" />
              ))}
            </Rows>
            <Rule label="Stat" className="mt-12" />
            <div className="mt-6 grid gap-x-8 gap-y-10 sm:grid-cols-2">
              {statStates.map(s => (
                <Stat key={s} state={s} figure="42" label={`state: ${s}`} size="title" />
              ))}
            </div>
          </div>
        </div>
      </Band>

      <Band tone="sage" aria-labelledby="engine-title">
        <SectionHead kicker="03 / Engine" title="Agents share one governor." id="engine-title">
          <p className="m-0">Agents share a resource governor instead of creating a shell each. These are the default limits.</p>
        </SectionHead>
        <div className="mt-16 grid gap-x-12 gap-y-14 lg:grid-cols-12">
          <Facts className="lg:col-span-6 lg:self-start">
            <Fact label="license">MIT</Fact>
            <Fact label="written in">Rust, with TypeScript and Python SDKs</Fact>
            <Fact label="status">Source release, version 0.1.0</Fact>
            <Fact label="platform">macOS on Apple Silicon for ejm-ide</Fact>
          </Facts>
          <div className="grid gap-x-8 gap-y-10 sm:grid-cols-2 lg:col-span-6">
            <Stat figure="100" label="managed sessions" />
            <Stat figure="8" label="model requests" />
            <Stat figure="4" label="local jobs" />
            <Stat figure="1" label="heavy job" />
          </div>
        </div>
      </Band>

      <Band tone="ink" aria-labelledby="open-title">
        <SectionHead kicker="04 / Open source" title={<><span className="block">Read it.</span><span className="block">Build it.</span><span className="block">Change it.</span></>} id="open-title">
          <p className="m-0">ejm-cli, ejm-ide and ejm-harness are open source under the MIT license.</p>
          <p className="mt-8 mb-0"><Button asChild arrow><a href={GITHUB} rel="external">ejmlabs on GitHub</a></Button></p>
        </SectionHead>
      </Band>
    </>
  );
}
