import { Band, Facts, Fact, SectionHead } from '@ejm/ui';
import { SectionBody } from '@ejm/ui/layout/SiteContent';

export default function About() {
  return (
    <Band>
      <SectionHead as="h1" kicker="About" title="What is in the starter" purpose="page">
        One small site that uses every shared part, so you can copy it.
      </SectionHead>
      <SectionBody><Facts>
        <Fact label="Runtime">Real React 19, prerendered to static HTML by @ejm/ssg and hydrated on load.</Fact>
        <Fact label="Look">@ejm/ui layouts on the @ejm/tokens palette and the two brand fonts.</Fact>
        <Fact label="Shared">Analytics, head, theme boot and security headers from @ejm/site.</Fact>
      </Facts></SectionBody>
    </Band>
  );
}
