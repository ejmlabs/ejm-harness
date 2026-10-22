import type { AnchorHTMLAttributes } from 'react';
import { ExternalLink } from '../shell/ExternalLink';
import { useLinkComponent } from '../shell/link';

/** Inline links use the site's public routing adapter and one underline treatment. */
export function InlineLink({ external = false, ...props }: Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'className' | 'style' | 'dangerouslySetInnerHTML'> & { href: string; external?: boolean }) {
  const InternalLink = useLinkComponent();
  const Link = external ? ExternalLink : InternalLink;
  return <Link {...props} className="link" />;
}
