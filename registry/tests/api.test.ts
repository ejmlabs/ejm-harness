import { env, exports } from "cloudflare:workers";
import { applyD1Migrations, reset } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import fixture from "./fixtures/archives.json" with { type: "json" };

interface Bindings {
  DB: D1Database;
  PACKAGES: R2Bucket;
  REGISTRY_MIGRATIONS: { name: string; queries: string[] }[];
}
const bindings = env as unknown as Bindings;
const origin = "https://registry.example.invalid";
const versionPath = "/v1/packages/alice/test/versions/1.0.0";
const archive = Uint8Array.from(atob(fixture.base64), (character) => character.charCodeAt(0));
const registry = exports as unknown as {
  default: { fetch(url: string, init: RequestInit): Promise<Response> };
};
let ip = 0;

function call(path: string, init: RequestInit = {}, token?: string): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set("cf-connecting-ip", `192.0.2.${String(++ip)}`);
  if (token) headers.set("authorization", `Bearer ${token}`);
  return registry.default.fetch(`${origin}${path}`, { ...init, headers });
}

function reserve(token = "alice-token", extra = {}): Promise<Response> {
  return call(
    "/v1/packages/alice/test/versions",
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        manifest: fixture.manifest,
        sha256: fixture.sha256,
        size: fixture.size,
        ...extra,
      }),
    },
    token,
  );
}

function upload(bytes = archive, token = "alice-token"): Promise<Response> {
  return call(`${versionPath}/archive`, { method: "PUT", body: bytes }, token);
}

beforeEach(async () => {
  await reset();
  await applyD1Migrations(bindings.DB, bindings.REGISTRY_MIGRATIONS);
  await bindings.DB.prepare(
    "INSERT INTO registry_accounts(id) VALUES ('alice'),('bob'),('operator')",
  ).run();
  await bindings.DB.prepare(
    "INSERT INTO publishers(handle,user_id,display_name) VALUES ('alice','alice','Alice')",
  ).run();
});

describe("registry publication boundary", () => {
  it("hides reservations, verifies uploads, and serves only ready immutable bytes", async () => {
    expect((await reserve()).status).toBe(200);
    expect((await call(versionPath)).status).toBe(404);
    expect((await call(`${versionPath}/download`)).status).toBe(404);
    expect((await upload(Uint8Array.of(1, 2, 3))).status).toBe(400);
    const published = await upload();
    expect(published.status).toBe(200);
    expect(await published.json()).toMatchObject({
      status: "ready",
      sha256: fixture.sha256,
      size: fixture.size,
    });
    const downloaded = await call(`${versionPath}/download`);
    expect(downloaded.status).toBe(200);
    expect(downloaded.headers.get("cache-control")).toBe("no-store");
    expect(new Uint8Array(await downloaded.arrayBuffer())).toEqual(archive);
    expect((await reserve("alice-token", { sha256: "f".repeat(64) })).status).toBe(409);
    expect((await upload()).status).toBe(200);
  });

  it("blocks namespace takeover and unauthorized reservation or upload", async () => {
    expect((await reserve("bob-token")).status).toBe(403);
    expect((await reserve()).status).toBe(200);
    expect((await upload(archive, "bob-token")).status).toBe(403);
    const takeover = await call(
      "/v1/publishers",
      {
        method: "POST",
        body: JSON.stringify({ handle: "alice", display_name: "Bob" }),
      },
      "bob-token",
    );
    expect(takeover.status).toBe(409);
    expect(
      (
        await call(
          "/v1/operator/accounts/alice",
          {
            method: "POST",
            body: JSON.stringify({ disabled: true, reason: "forged operator" }),
            headers: { "x-operator": "true" },
          },
          "bob-token",
        )
      ).status,
    ).toBe(403);
  });

  it("rejects cross-origin cookies and never falls back from an invalid bearer", async () => {
    for (const headers of [
      { cookie: "session=anything" },
      { cookie: "session=anything", origin: "https://evil.example" },
      { cookie: "session=anything", authorization: "Basic forged", origin },
      { authorization: "Bearer alice-token", origin: "null" },
    ]) {
      const response = await call("/v1/publishers", { method: "POST", headers, body: "{}" });
      expect(response.status).toBe(403);
    }
    expect(
      (
        await call("/v1/me", {
          headers: { authorization: "Basic forged", cookie: "session=anything" },
        })
      ).status,
    ).toBe(401);
  });

  it("repairs an interrupted R2 write without replacing the object", async () => {
    await reserve();
    await bindings.PACKAGES.put("alice/test/1.0.0.tar.gz", archive, {
      customMetadata: { sha256: fixture.sha256 },
    });
    const before = await bindings.PACKAGES.head("alice/test/1.0.0.tar.gz");
    expect((await upload()).status).toBe(200);
    expect((await bindings.PACKAGES.head("alice/test/1.0.0.tar.gz"))?.version).toBe(
      before?.version,
    );
  });

  it("serializes concurrent immutable uploads", async () => {
    await reserve();
    const responses = await Promise.all([upload(), upload()]);
    expect(responses.map((response) => response.status)).toEqual([200, 200]);
    expect((await bindings.PACKAGES.list()).objects).toHaveLength(1);
  });

  it.each(["linked_archive", "ancestor_conflict_archive"] as const)(
    "rejects a malformed %s archive before R2 publication",
    async (key) => {
      const bytes = Uint8Array.from(atob(fixture[key]), (character) => character.charCodeAt(0));
      const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
      const sha256 = Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
      expect((await reserve("alice-token", { sha256, size: bytes.length })).status).toBe(200);
      expect((await upload(bytes)).status).toBe(400);
      expect((await bindings.PACKAGES.list()).objects).toHaveLength(0);
    },
  );

  it("compares the archive manifest to the immutable reservation", async () => {
    await reserve("alice-token", {
      manifest: { ...fixture.manifest, description: "Changed reservation" },
    });
    expect((await upload()).status).toBe(400);
    expect((await call(versionPath)).status).toBe(404);
  });

  it("rejects body overflows before parsing or storage", async () => {
    expect((await reserve()).status).toBe(200);
    expect((await upload(new Uint8Array(fixture.size + 1))).status).toBe(413);
    const response = await call(
      "/v1/packages/alice/test/versions",
      {
        method: "POST",
        body: "x".repeat(18 * 1024),
      },
      "alice-token",
    );
    expect(response.status).toBe(413);
  });

  it("hides quarantined and suspended publications and audits operator changes", async () => {
    await reserve();
    await upload();
    const quarantine = await call(
      "/v1/operator/versions/alice/test/1.0.0",
      {
        method: "POST",
        body: JSON.stringify({ disabled: true, reason: "Local abuse fixture" }),
      },
      "operator-token",
    );
    expect(quarantine.status).toBe(200);
    expect((await call(`${versionPath}/download`)).status).toBe(404);
    expect((await reserve()).status).toBe(409);
    expect(
      (
        await bindings.DB.prepare("SELECT count(*) AS count FROM moderation_log").first<{
          count: number;
        }>()
      )?.count,
    ).toBe(1);
    await bindings.DB.prepare("UPDATE versions SET quarantined=0").run();
    await bindings.DB.prepare("UPDATE publishers SET suspended=1 WHERE handle='alice'").run();
    expect((await call(versionPath)).status).toBe(404);
    expect((await upload()).status).toBe(403);
    expect(await (await call("/v1/packages")).json()).toMatchObject({ packages: [] });
  });

  it.each(["daily_upload_bytes", "daily_upload_count"] as const)(
    "enforces the global %s budget across different subjects",
    async (column) => {
      await reserve();
      await bindings.DB.prepare(`UPDATE registry_limits SET ${column}=?`)
        .bind(column === "daily_upload_bytes" ? fixture.size : 1)
        .run();
      expect((await upload()).status).toBe(200);
      expect(
        (
          await call(
            "/v1/publishers",
            { method: "POST", body: JSON.stringify({ handle: "bob", display_name: "Bob" }) },
            "bob-token",
          )
        ).status,
      ).toBe(200);
      expect(
        (
          await call(
            "/v1/packages/bob/test/versions",
            {
              method: "POST",
              body: JSON.stringify({
                manifest: { ...fixture.manifest, name: "@bob/test" },
                sha256: fixture.sha256,
                size: fixture.size,
              }),
            },
            "bob-token",
          )
        ).status,
      ).toBe(200);
      const blocked = await call(
        "/v1/packages/bob/test/versions/1.0.0/archive",
        { method: "PUT", body: archive },
        "bob-token",
      );
      expect(blocked.status).toBe(429);
      expect(await blocked.json()).toMatchObject({
        error: { code: "registry_upload_budget_reached" },
      });
      expect((await bindings.PACKAGES.list()).objects).toHaveLength(1);
    },
  );

  it("does not expose the private session path through the public Worker", async () => {
    expect((await call("/internal/session", {}, "operator-token")).status).toBe(404);
    expect((await call("/api/auth/internal/session", {}, "operator-token")).status).toBe(403);
  });

  it("bounds reservation rates with an atomic D1 counter", async () => {
    const responses = [];
    for (let attempt = 0; attempt < 21; attempt++) responses.push((await reserve()).status);
    expect(responses.slice(0, 20)).toEqual(Array(20).fill(200));
    expect(responses[20]).toBe(429);
  });

  it("keeps full manifest pages below the native JSON bound", async () => {
    for (let index = 0; index < 4; index++) {
      const version = `1.0.${String(index)}`;
      const manifest = {
        ...fixture.manifest,
        version,
        runtime: { kind: "node", entry: "index.js" },
        exports: {
          tools: [
            {
              name: "fixture",
              description: "Fixture",
              input_schema: { type: "object", description: "x".repeat(14 * 1024) },
              effect: "read",
              version: "1.0.0",
              handler: "fixture",
            },
          ],
        },
      };
      await bindings.DB.prepare(
        "INSERT INTO versions(publisher,name,version,manifest_json,description,sha256,size,ready) VALUES ('alice','test',?,?, 'Fixture',?,1,1)",
      )
        .bind(version, JSON.stringify(manifest), "a".repeat(64))
        .run();
    }
    const first = await call("/v1/packages/alice/test");
    const bytes = await first.text();
    expect(new TextEncoder().encode(bytes).length).toBeLessThan(64 * 1024);
    expect(JSON.parse(bytes)).toMatchObject({ next_cursor: "1.0.2" });
    const second = await call("/v1/packages/alice/test?cursor=1.0.2");
    expect(await second.json()).toMatchObject({
      next_cursor: null,
      versions: [{ manifest: { version: "1.0.3" } }],
    });
  });
});
