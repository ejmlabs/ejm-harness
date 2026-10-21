import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { ejmSite } from "@ejm/site/vite";
import { ejmSsg } from "@ejm/ssg/vite";
import { markdown } from "./vite/markdown.mjs";

// ejmSite adds the shared head tags and writes theme-boot.js, the icons and _headers. ejmSsg prerenders every route.
// markdown() turns site/content/*.md into data for the ejm-ui docs components, in Node, so no parser is shipped.
// The linked @ejm packages would otherwise bring a second React from ejm-ui/node_modules, so React is deduped.
export default defineConfig({
  publicDir: false,
  resolve: { dedupe: ["react", "react-dom"] },
  plugins: [
    markdown(),
    tailwindcss(),
    react(),
    ejmSite({ project: "harness" }),
    ejmSsg({ site: "src/site.tsx" }),
  ],
  build: { target: "es2022", cssMinify: "lightningcss", sourcemap: true },
});
