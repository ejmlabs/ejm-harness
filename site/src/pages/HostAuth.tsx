import { Static } from "@ejm/ssg";
import { Contents, DocHead, DocsLayout, DocsSidebar } from "@ejm/ui";
import { SectionBody } from "@ejm/ui/layout/SiteContent";
import { contents, head } from "./host-auth-head";

const Guide =
    import.meta.env.SSR || import.meta.env.DEV
        ? (await import("./HostAuthGuide")).default
        : undefined;

const docs = [
    {
        title: "ejm-harness",
        items: [
            { label: "Harness home", href: "/" },
            { label: "Host authentication", href: "/docs/host-auth/" },
        ],
    },
];

/** The guide text is server-rendered Static HTML, so the prose costs no script. */
export default function HostAuth() {
    return (
        <DocsLayout
            sidebar={<DocsSidebar groups={docs} current="/docs/host-auth/" />}
            contents={<Contents items={contents} />}
        >
            <article>
                <DocHead page={{ ...head, sections: [] }} />
                <SectionBody>
                    <Static render={Guide ? () => <Guide /> : undefined} />
                </SectionBody>
            </article>
        </DocsLayout>
    );
}
