import { Link, usePath } from "@ejm/ssg";
import { Analytics } from "@ejm/site/analytics";
import { otherSites, PROJECTS, sourceLinks } from "@ejm/site/projects";
import { SiteLayout, type FooterProps, type NavItem } from "@ejm/ui";
import type { ReactNode } from "react";
import markInk from "@ejm/tokens/assets/projects/harness/mark-ink.svg?url";
import markPaper from "@ejm/tokens/assets/projects/harness/mark-paper.svg?url";

const here = PROJECTS.harness;
const identity = {
    name: here.name,
    artwork: { src: markInk, darkSrc: markPaper, width: 256, height: 256 },
};

const pages = [
    { label: "Home", href: "/" },
    { label: "Host authentication", href: "/docs/host-auth/" },
];

const footer: FooterProps = {
    identity: { ...identity, description: here.tagline },
    columns: [
        { heading: "Pages", links: pages },
        { heading: "Sites", links: otherSites("harness") },
        { heading: "Source", links: sourceLinks("harness") },
    ],
    privacy:
        "Visits are counted with Cloudflare Web Analytics, which sets no cookies and is off when your browser sends Do Not Track or Global Privacy Control. The only cookie is your theme choice.",
    legal: "© 2026 ejmlabs. ejm-harness is released under the MIT license.",
};

/** The page frame for every route: skip link, header with the theme toggle last, the page, the footer, and analytics. */
export function App({ children }: { children: ReactNode }) {
    const path = usePath();
    const nav: NavItem[] = pages.map((p) => ({
        ...p,
        current: p.href === "/" ? path === "/" : path.startsWith(p.href),
    }));
    return (
        <>
            <SiteLayout
                header={{ identity, homeLabel: `${here.name} home`, nav }}
                footer={footer}
                linkComponent={Link}
            >
                {children}
            </SiteLayout>
            <Analytics />
        </>
    );
}
