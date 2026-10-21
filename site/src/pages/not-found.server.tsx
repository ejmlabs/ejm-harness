// Static recovery copy stays in the built 404 document.
import { Band } from "@ejm/ui/layout/Band";
import { SectionHead } from "@ejm/ui/layout/SectionHead";
import { InlineLink } from "@ejm/ui/layout/InlineLink";

export default function NotFoundContent() {
    return (
        <Band tone="paper" aria-labelledby="nf-title">
            <SectionHead
                kicker="404"
                title="That page is not here."
                id="nf-title"
                as="h1"
                purpose="page"
            >
                <p>
                    The page may have moved. <InlineLink href="/docs/">The docs</InlineLink> list
                    every guide, and <InlineLink href="/">the home page</InlineLink> starts at the
                    top.
                </p>
            </SectionHead>
        </Band>
    );
}
