import { readFileSync } from "node:fs";
import { URL } from "node:url";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";

export default defineConfig(async () => ({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.jsonc" },
      miniflare: {
        d1Databases: ["DB", "AUTH_DB"],
        bindings: {
          REGISTRY_ORIGIN: "https://registry.example.invalid",
          REGISTRY_MIGRATIONS: await readD1Migrations("./migrations"),
          AUTH_MIGRATIONS: await readD1Migrations("./auth/migrations"),
        },
        serviceBindings: { AUTH: "registry-fixture-auth" },
        workers: [
          {
            name: "registry-fixture-auth",
            modules: true,
            script: readFileSync(
              new URL("./tests/fixtures/auth-service.js", import.meta.url),
              "utf8",
            ),
            compatibilityDate: "2026-10-18",
          },
        ],
      },
    }),
  ],
  test: { include: ["tests/**/*.test.ts"], fileParallelism: false },
}));
