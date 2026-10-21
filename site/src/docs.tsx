import { Static } from "@ejm/ssg";
import {
    DocsPage as SharedDocsPage,
    docNavigation,
    type ClientDocPage,
    type FullDocPage,
    type DocProseRun,
    type DocsPageProps,
} from "@ejm/ui/docs";
import pages from "virtual:docs-meta";

const DocProse =
    import.meta.env.SSR || import.meta.env.DEV
        ? (await import("@ejm/ui/docs/DocProse")).DocProse
        : undefined;
const CodeHighlight =
    import.meta.env.SSR || import.meta.env.DEV
        ? (await import("@ejm/ui/docs/CodeHighlight")).CodeHighlight
        : undefined;

/** Only product page order and route paths belong in this adapter. */
export function DocsPage({
    client,
    full,
    renderBlock,
}: {
    client: ClientDocPage;
    full?: FullDocPage;
    renderBlock?: DocsPageProps["renderBlock"];
}) {
    const href = client.slug ? `/docs/${client.slug}/` : "/docs/";
    return (
        <SharedDocsPage
            page={client}
            {...docNavigation(pages, href)}
            staticBoundary={Static}
            renderBlock={renderBlock}
            renderCode={
                CodeHighlight
                    ? (block) => (
                          <CodeHighlight code={block.code} lang={block.lang} label={block.label} />
                      )
                    : undefined
            }
            renderProse={
                DocProse && full
                    ? (si, ii) => (
                          <DocProse
                              section={full.sections[si]!}
                              blocks={(full.sections[si]!.items[ii] as DocProseRun).blocks}
                              first={ii === 0}
                          />
                      )
                    : undefined
            }
        />
    );
}
