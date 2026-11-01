import { defineSite } from "@ejm/ssg";
import { siteDefaults } from "@ejm/site/head";
import HostAuth from "./pages/HostAuth";
import Home from "./pages/Home";
import NotFound from "./pages/NotFound";
import { App } from "./shell";
import "./styles.css";

export default defineSite({
    ...siteDefaults("harness", {
        description:
            "ejm-harness is a reusable Rust engine for coding agents. Run many sessions through one resource governor, keep a durable event journal, and extend the engine from Rust, TypeScript or Python.",
    }),
    App,
    routes: [
        { path: "/", component: Home, head: { title: "ejm-harness" } },
        {
            path: "/docs/host-auth/",
            component: HostAuth,
            head: {
                title: "Host authentication",
                description:
                    "Configure protected credentials, migrate registry sessions, use the official Claude Code provider and change providers safely in a Rust host.",
            },
        },
    ],
    notFound: { component: NotFound, head: { title: "Page not found" } },
});
