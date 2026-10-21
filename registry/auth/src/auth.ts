import { betterAuth, type BetterAuthOptions } from "better-auth";
import { bearer, deviceAuthorization } from "better-auth/plugins";
import type { DatabaseSync } from "node:sqlite";
import { atomicDeviceConsent } from "./consent.js";

export interface Env {
  DB: D1Database;
  REGISTRY_ORIGIN: string;
  BETTER_AUTH_SECRET: string;
  GITHUB_CLIENT_ID: string;
  GITHUB_CLIENT_SECRET: string;
  OPERATOR_SUBJECT_IDS?: string;
}

export function authOptions(
  env: Omit<Env, "DB">,
  database: D1Database | DatabaseSync,
): BetterAuthOptions {
  return {
    database,
    appName: "ejm Registry",
    baseURL: env.REGISTRY_ORIGIN,
    basePath: "/api/auth",
    secret: env.BETTER_AUTH_SECRET,
    trustedOrigins: [env.REGISTRY_ORIGIN],
    socialProviders: {
      github: {
        clientId: env.GITHUB_CLIENT_ID,
        clientSecret: env.GITHUB_CLIENT_SECRET,
      },
    },
    account: { accountLinking: { enabled: false } },
    session: {
      expiresIn: 60 * 60 * 24 * 7,
      updateAge: 60 * 60 * 24,
      cookieCache: { enabled: false },
    },
    advanced: {
      useSecureCookies: true,
      ipAddress: { ipAddressHeaders: ["cf-connecting-ip"] },
      defaultCookieAttributes: { httpOnly: true, secure: true, sameSite: "lax" },
    },
    rateLimit: {
      enabled: true,
      storage: "database",
      window: 60,
      max: 60,
      customRules: {
        "/device/code": { window: 60, max: 10 },
        "/device/token": { window: 60, max: 30 },
        "/sign-in/social": { window: 60, max: 10 },
      },
    },
    logger: { disabled: true },
    plugins: [
      bearer(),
      deviceAuthorization({
        expiresIn: "10m",
        interval: "5s",
        verificationUri: `${env.REGISTRY_ORIGIN}/device`,
        validateClient: (clientId: string) => clientId === "ejm-cli",
      }),
      atomicDeviceConsent,
    ],
  } satisfies BetterAuthOptions;
}

export function createAuth(env: Env): ReturnType<typeof betterAuth<BetterAuthOptions>> {
  return betterAuth(authOptions(env, env.DB));
}
