import { env } from "cloudflare:workers";
import { applyD1Migrations, reset } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import authWorker, { credentialHeaders, requestAllowed } from "../auth/src/index.js";
import { createAuth, type Env } from "../auth/src/auth.js";

interface Bindings {
  AUTH_DB: D1Database;
  AUTH_MIGRATIONS: { name: string; queries: string[] }[];
}
const bindings = env as unknown as Bindings;
const origin = "https://registry.example.invalid";
const authEnv: Env = {
  DB: bindings.AUTH_DB,
  REGISTRY_ORIGIN: origin,
  BETTER_AUTH_SECRET: "local-fixture-only-secret-at-least-thirty-two-characters",
  GITHUB_CLIENT_ID: "local-fixture",
  GITHUB_CLIENT_SECRET: "local-fixture",
  OPERATOR_SUBJECT_IDS: "",
};

function call(
  path: string,
  method = "GET",
  body?: unknown,
  headers?: HeadersInit,
): Promise<Response> {
  const requestHeaders = new Headers(headers);
  requestHeaders.set("cf-connecting-ip", "192.0.2.200");
  if (body !== undefined) requestHeaders.set("content-type", "application/json");
  return authWorker.fetch(
    new Request(`${origin}${path}`, {
      method,
      headers: requestHeaders,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    }),
    authEnv,
  );
}

async function session(
  label = "fixture",
): Promise<{ user: { id: string }; session: { token: string } }> {
  const context = await createAuth(authEnv).$context;
  const user = await context.internalAdapter.createUser(
    {
      email: `${label}@example.invalid`,
      name: "Fixture",
      emailVerified: true,
    },
    { method: "api" },
  );
  const session = await context.internalAdapter.createSession(user.id);
  return { user, session };
}

beforeEach(async () => {
  await reset();
  await applyD1Migrations(bindings.AUTH_DB, bindings.AUTH_MIGRATIONS);
});

describe("private Better Auth boundary", () => {
  it("grants operator status only to exact configured subjects", async () => {
    const { user, session: active } = await session();
    const request = new Request(`${origin}/internal/session`, {
      headers: { authorization: `Bearer ${active.token}` },
    });
    const allowed = await authWorker.fetch(request, { ...authEnv, OPERATOR_SUBJECT_IDS: user.id });
    expect(await allowed.json()).toMatchObject({ is_operator: true });
    const rejected = await authWorker.fetch(request, {
      ...authEnv,
      OPERATOR_SUBJECT_IDS: `${user.id}-suffix`,
    });
    expect(await rejected.json()).toMatchObject({ is_operator: false });
  });

  it("enforces pending, polling, client, and expiration boundaries", async () => {
    const created = await call("/api/auth/device/code", "POST", {
      client_id: "ejm-cli",
      scope: "registry",
    });
    const grant = await created.json<{ device_code: string }>();
    const body = {
      client_id: "ejm-cli",
      device_code: grant.device_code,
      grant_type: "urn:ietf:params:oauth:grant-type:device_code",
    };
    for (const clientId of ["attacker", "unsupported-cli"])
      expect(
        (await call("/api/auth/device/token", "POST", { ...body, client_id: clientId })).status,
      ).toBe(400);
    const pending = await call("/api/auth/device/token", "POST", body);
    expect(await pending.json()).toMatchObject({ error: "authorization_pending" });
    const tooFast = await call("/api/auth/device/token", "POST", body);
    expect(await tooFast.json()).toMatchObject({ error: "slow_down" });
    await bindings.AUTH_DB.prepare('UPDATE "deviceCode" SET "expiresAt"=?,"lastPolledAt"=NULL')
      .bind(new Date(0).toISOString())
      .run();
    const expired = await call("/api/auth/device/token", "POST", body);
    expect(await expired.json()).toMatchObject({ error: "expired_token" });
  });

  it("binds verification to one user and honors device denial", async () => {
    const { session: owner } = await session("owner");
    const { session: other } = await session("other");
    const created = await call("/api/auth/device/code", "POST", {
      client_id: "ejm-cli",
      scope: "registry",
    });
    const grant = await created.json<{ user_code: string; device_code: string }>();
    const path = `/api/auth/device?user_code=${encodeURIComponent(grant.user_code)}`;
    expect(
      (await call(path, "GET", undefined, { authorization: `Bearer ${owner.token}` })).status,
    ).toBe(200);
    const otherView = await call(path, "GET", undefined, {
      authorization: `Bearer ${other.token}`,
    });
    expect(otherView.status).toBe(200);
    const otherData = await otherView.json<Record<string, unknown>>();
    expect(otherData).not.toHaveProperty("client_id");
    expect(otherData).not.toHaveProperty("scope");
    expect(
      (
        await call(
          "/api/auth/device/approve",
          "POST",
          { userCode: grant.user_code },
          { authorization: `Bearer ${other.token}`, origin },
        )
      ).status,
    ).toBeGreaterThanOrEqual(400);
    expect(
      (
        await call(
          "/api/auth/device/deny",
          "POST",
          { userCode: grant.user_code },
          { authorization: `Bearer ${owner.token}`, origin },
        )
      ).status,
    ).toBe(200);
    const denied = await call("/api/auth/device/token", "POST", {
      client_id: "ejm-cli",
      device_code: grant.device_code,
      grant_type: "urn:ietf:params:oauth:grant-type:device_code",
    });
    expect(await denied.json()).toMatchObject({ error: "access_denied" });
  });

  it("rejects CSRF, public internal paths, and bearer cookie fallback", () => {
    expect(requestAllowed(new Request(`${origin}/internal/session`), origin)).toBe(false);
    expect(
      requestAllowed(new Request(`${origin}/api/auth/device/approve`, { method: "POST" }), origin),
    ).toBe(false);
    expect(
      requestAllowed(
        new Request(`${origin}/api/auth/device/approve`, {
          method: "POST",
          headers: { origin: "https://evil.example" },
        }),
        origin,
      ),
    ).toBe(false);
    expect(
      credentialHeaders(new Headers({ authorization: "Basic forged", cookie: "valid=maybe" })),
    ).toBeNull();
    const headers = credentialHeaders(
      new Headers({ authorization: "Bearer token", cookie: "valid=maybe" }),
    );
    expect(headers?.has("cookie")).toBe(false);
    expect(headers?.get("authorization")).toBe("Bearer token");
  });

  it("commits only one terminal decision for concurrent consent requests", async () => {
    const { session: active } = await session();
    const authorization = `Bearer ${active.token}`;
    const created = await call("/api/auth/device/code", "POST", {
      client_id: "ejm-cli",
      scope: "registry",
    });
    const grant = await created.json<{ user_code: string }>();
    await call(
      `/api/auth/device?user_code=${encodeURIComponent(grant.user_code)}`,
      "GET",
      undefined,
      {
        authorization,
      },
    );
    const decisions = await Promise.all(
      ["deny", "approve"].map((action) =>
        call(
          `/api/auth/device/${action}`,
          "POST",
          { userCode: grant.user_code },
          { authorization, origin },
        ),
      ),
    );
    expect(decisions.filter((response) => response.status === 200)).toHaveLength(1);
  });

  it("allows only the first-party device client and known access label", async () => {
    for (const body of [
      { client_id: "attacker", scope: "registry" },
      { client_id: "unsupported-cli", scope: "registry" },
      { client_id: "ejm-cli", scope: "admin" },
      { client_id: "ejm-cli", scope: "registry", user_id: "victim" },
    ])
      expect((await call("/api/auth/device/code", "POST", body)).status).toBe(400);
    const response = await call("/api/auth/device/code", "POST", {
      client_id: "ejm-cli",
      scope: "registry",
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({
      interval: 5,
      expires_in: 600,
      verification_uri: `${origin}/device`,
    });
  });

  it("returns only normalized identity and checks database revocation", async () => {
    const { user, session: active } = await session();
    const authorization = `Bearer ${active.token}`;
    const response = await call("/internal/session", "GET", undefined, {
      authorization,
      "x-operator": "true",
    });
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      id: user.id,
      display_name: "Fixture",
      is_operator: false,
    });
    const context = await createAuth(authEnv).$context;
    await context.internalAdapter.deleteSession(active.token);
    expect((await call("/internal/session", "GET", undefined, { authorization })).status).toBe(401);
  });

  it("revokes other sessions and signs out through the exposed auth endpoints", async () => {
    const { user, session: active } = await session();
    const other = await (await createAuth(authEnv).$context).internalAdapter.createSession(user.id);
    const authorization = `Bearer ${active.token}`;
    expect(
      (await call("/api/auth/revoke-other-sessions", "POST", {}, { authorization })).status,
    ).toBe(403);
    expect(
      (await call("/api/auth/revoke-other-sessions", "POST", {}, { authorization, origin })).status,
    ).toBe(200);
    expect(
      (
        await call("/internal/session", "GET", undefined, {
          authorization: `Bearer ${other.token}`,
        })
      ).status,
    ).toBe(401);
    expect((await call("/internal/session", "GET", undefined, { authorization })).status).toBe(200);
    expect((await call("/api/auth/sign-out", "POST", {}, { authorization })).status).toBe(200);
    expect((await call("/internal/session", "GET", undefined, { authorization })).status).toBe(401);
  });

  it("requires ownership and consumes concurrent device redemption once", async () => {
    const { session: active } = await session();
    const authorization = `Bearer ${active.token}`;
    const created = await call("/api/auth/device/code", "POST", {
      client_id: "ejm-cli",
      scope: "registry",
    });
    const grant = await created.json<{ user_code: string; device_code: string }>();
    expect(
      (await call("/api/auth/device/approve", "POST", { userCode: grant.user_code }, { origin }))
        .status,
    ).toBe(401);
    const verified = await call(
      `/api/auth/device?user_code=${encodeURIComponent(grant.user_code)}`,
      "GET",
      undefined,
      { authorization },
    );
    expect(verified.status).toBe(200);
    expect(await verified.json()).toMatchObject({
      client_id: "ejm-cli",
      scope: "registry",
      status: "pending",
    });
    expect(
      (
        await call(
          "/api/auth/device/approve",
          "POST",
          { userCode: grant.user_code },
          { authorization, origin },
        )
      ).status,
    ).toBe(200);
    const redemptions = await Promise.all(
      Array.from({ length: 2 }, () =>
        call("/api/auth/device/token", "POST", {
          client_id: "ejm-cli",
          device_code: grant.device_code,
          grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        }),
      ),
    );
    const redeemed = redemptions.find((response) => response.status === 200);
    expect(redemptions.filter((response) => response.status === 200)).toHaveLength(1);
    if (!redeemed) throw new Error("Device redemption did not issue a session");
    const token = await redeemed.json<{ access_token: string }>();
    expect(
      (
        await call("/internal/session", "GET", undefined, {
          authorization: `Bearer ${token.access_token}`,
        })
      ).status,
    ).toBe(200);
    const replay = await call("/api/auth/device/token", "POST", {
      client_id: "ejm-cli",
      device_code: grant.device_code,
      grant_type: "urn:ietf:params:oauth:grant-type:device_code",
    });
    expect(replay.status).toBeGreaterThanOrEqual(400);
    await (await createAuth(authEnv).$context).internalAdapter.deleteSession(token.access_token);
    expect(
      (
        await call("/internal/session", "GET", undefined, {
          authorization: `Bearer ${token.access_token}`,
        })
      ).status,
    ).toBe(401);
  });
});
