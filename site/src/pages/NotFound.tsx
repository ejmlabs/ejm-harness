import { Hero } from "@ejm/ui";
import { InlineLink } from "@ejm/ui/layout/SiteContent";

export default function NotFound() {
    return (
        <Hero
            eyebrow="404"
            title="This page is not here"
            actions={<InlineLink href="/">Back to the ejm-harness home page</InlineLink>}
        />
    );
}
