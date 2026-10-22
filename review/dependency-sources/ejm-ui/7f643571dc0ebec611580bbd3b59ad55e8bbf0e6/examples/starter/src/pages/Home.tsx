import { Band, Hero, Row, Rows, SectionHead } from '@ejm/ui';
import { InlineLink, SectionBody } from '@ejm/ui/layout/SiteContent';
import { PROJECT_LIST } from '@ejm/site/projects';

const sites = PROJECT_LIST.filter(p => p.id !== 'ejmlabs');

export default function Home() {
  return (
    <>
      <Hero
        eyebrow="ejmlabs / starter"
        title="A small site on ejm-ui"
        actions={<InlineLink href="/about/">What is in the starter</InlineLink>}
      >
        Tokens, a type-led layout, a theme toggle that follows you across every ejmlabs site, and routes that are plain HTML before any script runs.
      </Hero>
      <Band tone="mist">
        <SectionHead kicker="01 / Sites" title="The rest of ejmlabs">
          Each site is built the same way and shares the theme, the header and the privacy rules.
        </SectionHead>
        <SectionBody><Rows aria-label="ejmlabs sites">
          {sites.map(p => (
            <Row key={p.id} title={p.name} description={p.tagline} meta={p.host} href={p.url} external />
          ))}
        </Rows></SectionBody>
      </Band>
    </>
  );
}
