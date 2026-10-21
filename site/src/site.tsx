import { defineSite } from "@ejm/ssg";
import { siteDefaults } from "@ejm/site/head";
import pages from "virtual:docs-meta";
import NotFound from "./pages/NotFound";
import { App } from "./shell";
import "./styles.css";

const doc = (slug: string) => pages.find((p) => p.slug === slug)!;

// Every page is its own chunk, so a route pays only for itself.
export default defineSite({
    ...siteDefaults("harness", {
        description:
            "ejm-harness is an open source Rust engine for coding agents under MIT, with TypeScript and Python SDKs. Build from source, read the docs, or contribute.",
    }),
    App,
    routes: [
        {
            path: "/",
            load: () => import("./pages/Home"),
            head: { title: "A Rust engine for coding agents" },
        },
        {
            path: "/docs/",
            load: () => import("./pages/DocsHome"),
            head: { title: doc("").title, description: doc("").description },
        },
        {
            path: "/docs/user-guide/",
            load: () => import("./pages/UserGuide"),
            head: { title: doc("user-guide").title, description: doc("user-guide").description },
        },
        {
            path: "/docs/host-auth/",
            load: () => import("./pages/HostAuth"),
            head: { title: doc("host-auth").title, description: doc("host-auth").description },
        },
        {
            path: "/docs/developer-guide/",
            load: () => import("./pages/DeveloperGuide"),
            head: {
                title: doc("developer-guide").title,
                description: doc("developer-guide").description,
            },
        },
    ],
    notFound: { component: NotFound, head: { title: "Page not found" } },
});
