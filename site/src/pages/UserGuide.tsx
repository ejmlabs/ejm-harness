import client, { full, renderBlock } from "../../content/user-guide.md";
import { DocsPage } from "../docs";

const content = import.meta.env.SSR || import.meta.env.DEV;

export default function UserGuide() {
    return <DocsPage client={client} full={content ? full : undefined} renderBlock={renderBlock} />;
}
