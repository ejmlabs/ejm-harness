import { Link, usePath } from "@ejm/ssg";
import { Analytics } from "@ejm/site/analytics";
import { otherSites, PROJECTS, sourceLinks } from "@ejm/site/projects";
import type { FooterProps } from "@ejm/ui/shell/Footer";
import type { NavItem } from "@ejm/ui/shell/link";
import { SiteLayout } from "@ejm/ui/shell/SiteLayout";
import logo from "@ejm/tokens/assets/projects/harness/mark-ink.svg?url&no-inline";
import darkLogo from "@ejm/tokens/assets/projects/harness/mark-paper.svg?url&no-inline";
import type { ReactNode } from "react";
import pages from "virtual:docs-meta";

const here = PROJECTS.harness;

const [cli, ide] = otherSites("harness").filter((s) => s.label !== "ejmlabs");
const source = sourceLinks("harness")[0]!;

const footer: FooterProps = {
    identity: {
        name: here.name,
        description: "Open source Rust engine for coding agents.",
        artwork: { src: logo, darkSrc: darkLogo, width: 40, height: 40 },
    },
    columns: [
        {
            heading: "Docs",
            links: [
                ...pages.map((p) => ({ label: p.title, href: p.href })),
                { label: "Website privacy", href: "https://ejmlabs.com/privacy/", external: true },
            ],
        },
        { heading: "Sites", links: otherSites("harness") },
        {
            heading: "Source",
            links: [
                { label: "ejm-harness on GitHub", href: source.href, external: true },
                {
                    label: "Changelog",
                    href: `${source.href}/blob/main/CHANGELOG.md`,
                    external: true,
                },
                {
                    label: "Contribute",
                    href: `${source.href}/blob/main/CONTRIBUTING.md`,
                    external: true,
                },
                { label: "MIT license", href: `${source.href}/blob/main/LICENSE`, external: true },
            ],
        },
    ],
    privacy:
        "Visits are counted with Cloudflare Web Analytics, which sets no cookies and is off when your browser sends Do Not Track or Global Privacy Control. The only cookie is your theme choice.",
    legal: "ejm-harness is an open source ejmlabs project under the MIT license.",
};

// Shared links load prerendered documents when their route contains static prose.
/** The page frame for every route: skip link, header with the theme toggle last, the page, the footer, and analytics. */
export function App({ children }: { children: ReactNode }) {
    const path = usePath();
    const nav: NavItem[] = [
        { label: "Docs", href: "/docs/", current: path.startsWith("/docs") },
        { label: "Watch", href: "/#demo" },
        { label: cli!.label, href: cli!.href, external: true },
        { label: ide!.label, href: ide!.href, external: true },
        { label: "Source", href: source.href, external: true },
    ];
    return (
        <>
            <SiteLayout
                header={{
                    identity: {
                        name: here.name,
                        artwork: { src: logo, darkSrc: darkLogo, width: 40, height: 40 },
                    },
                    homeLabel: `${here.name} home`,
                    nav,
                }}
                footer={footer}
                linkComponent={Link}
            >
                {children}
            </SiteLayout>
            <Analytics />
        </>
    );
}
