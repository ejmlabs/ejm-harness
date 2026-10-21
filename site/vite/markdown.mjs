// Product source paths and route metadata. Shared packages compile and render all Markdown.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parseDoc, splitDoc } from "@ejm/site/docs";

const META = "virtual:docs-meta";
const options = {
  requireFrontMatter: true,
  requiredMetadata: ["title", "lead"],
  requireExplicitIds: true,
};

function docsMeta(dir) {
  const pages = readdirSync(dir)
    .filter((file) => file.endsWith(".md"))
    .map((file) => parseDoc(readFileSync(join(dir, file), "utf8"), options))
    .map(({ title, description, group, slug, order }) => ({
      title,
      description,
      group,
      slug,
      order,
      href: slug ? `/docs/${slug}/` : "/docs/",
    }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
  return `export default ${JSON.stringify(pages)};`;
}

/** Product Markdown imports expose prose for SSR and only live controls for the browser. */
export function markdown(contentDir = "content") {
  let dir = contentDir;
  return {
    name: "ejm-harness-markdown",
    enforce: "pre",
    configResolved(config) {
      dir = join(config.root, contentDir);
    },
    resolveId: (id) => (id === META ? `\0${META}` : null),
    load(id) {
      if (id !== `\0${META}`) return null;
      for (const file of readdirSync(dir)) this.addWatchFile(join(dir, file));
      return docsMeta(dir);
    },
    transform(source, id) {
      if (!id.endsWith(".md")) return null;
      const { full, client, requiresRich } = splitDoc(parseDoc(source, options));
      return {
        code: [
          requiresRich
            ? `import { createElement } from 'react';
import { DocDataBlock } from '@ejm/ui/docs/DocDataBlock';
export const renderBlock = (block, renderCode) => createElement(DocDataBlock, { block, renderCode });`
            : `export const renderBlock = undefined;`,
          `export const full = ${JSON.stringify(full)};`,
          `export default ${JSON.stringify(client)};`,
        ].join("\n"),
        map: null,
      };
    },
  };
}
