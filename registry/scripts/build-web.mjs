import { cp, copyFile, mkdir, readFile, readdir, rm, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import { checkNativeDesign } from "@ejm/site/native";

const root = fileURLToPath(new URL("..", import.meta.url));
const source = join(root, "public");
const output = join(root, ".web-assets");
const documents = await Promise.all(
  ["index", "account", "device"].map(async (name) => ({
    name: `${name}.html`,
    html: await readFile(join(source, `${name}.html`), "utf8"),
  })),
);
const checked = checkNativeDesign({
  root,
  source: ["public"],
  stylesheetHref: "/shared/ui.css",
  documents,
});
await rm(output, { recursive: true, force: true });
await cp(source, output, { recursive: true });
await mkdir(join(output, "shared", "fonts"), { recursive: true });
const assets = [
  ["@ejm/ui/static.css", "ui.css"],
  ["@ejm/ui/fonts/manrope-latin.woff2", "fonts/manrope-latin.woff2"],
  ["@ejm/ui/fonts/ibm-plex-mono-500-latin.woff2", "fonts/ibm-plex-mono-500-latin.woff2"],
  ["@ejm/ui/fonts/Manrope-OFL.txt", "fonts/Manrope-OFL.txt"],
  ["@ejm/ui/fonts/IBMPlexMono-OFL.txt", "fonts/IBMPlexMono-OFL.txt"],
  ["@ejm/tokens/assets/projects/harness/mark-sage.svg", "harness-mark.svg"],
  ["@ejm/tokens/assets/projects/harness/favicon.svg", "favicon.svg"],
];
for (const [specifier, destination] of assets) {
  await copyFile(
    fileURLToPath(import.meta.resolve(specifier)),
    join(output, "shared", destination),
  );
}
const css = gzipSync(await readFile(join(output, "shared/ui.css"))).length;
const fonts =
  (await stat(join(output, "shared/fonts/manrope-latin.woff2"))).size +
  (await stat(join(output, "shared/fonts/ibm-plex-mono-500-latin.woff2"))).size;
const scripts = (
  await Promise.all(
    (await readdir(source))
      .filter((name) => name.endsWith(".js"))
      .map(async (name) => gzipSync(await readFile(join(source, name))).length),
  )
).reduce((sum, size) => sum + size, 0);
if (css > 14 * 1024 || fonts > 70 * 1024 || scripts > 8 * 1024)
  throw new Error("Registry frontend exceeds its shared CSS, font or 8 KiB gzip script budget");
for (const document of documents) {
  if (gzipSync(document.html).length > 30 * 1024)
    throw new Error(`${document.name} exceeds the 30 KiB gzip HTML budget`);
}
console.log(
  `Native registry: ${checked.files.length} source files, ${checked.documents} pages; ${css} CSS gzip bytes, ${fonts} font bytes, ${scripts} script gzip bytes`,
);
