import { InlineLink } from '@ejm/ui/layout/SiteContent';
import { Band, SectionHead } from '@ejm/ui';

export default function NotFound() {
  return (
    <Band>
      <SectionHead as="h1" kicker="404" title="This page is not here" purpose="page">
        <InlineLink href="/">Back to the home page</InlineLink>
      </SectionHead>
    </Band>
  );
}
