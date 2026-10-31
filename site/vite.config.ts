import { defineConfig } from "vite";
import { ejmlabs } from "@ejm/site/vite";

export default defineConfig({ plugins: [ejmlabs({ project: "harness" })] });
