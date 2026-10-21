import client, { full, renderBlock } from "../../content/host-auth.md";
import { DocsPage } from "../docs";

const content = import.meta.env.SSR || import.meta.env.DEV;

export default function HostAuth() {
    return <DocsPage client={client} full={content ? full : undefined} renderBlock={renderBlock} />;
}
