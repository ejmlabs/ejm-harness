import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";

export default defineConfig(async () => ({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./auth/wrangler.jsonc" },
      miniflare: {
        d1Databases: ["DB", "AUTH_DB"],
        bindings: {
          REGISTRY_ORIGIN: "https://registry.example.invalid",
          AUTH_MIGRATIONS: await readD1Migrations("./auth/migrations"),
        },
      },
    }),
  ],
  test: { include: ["tests/auth.test.ts"], fileParallelism: false },
}));
