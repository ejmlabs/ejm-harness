import { createAuth, type Env } from "./auth.js";

const browserPosts = new Set([
  "/api/auth/sign-in/social",
  "/api/auth/device/approve",
  "/api/auth/device/deny",
  "/api/auth/revoke-other-sessions",
]);
const publicPosts = new Set([
  ...browserPosts,
  "/api/auth/sign-out",
  "/api/auth/device/code",
  "/api/auth/device/token",
]);
const publicGets = new Set([
  "/api/auth/get-session",
  "/api/auth/device",
  "/api/auth/callback/github",
  "/api/auth/error",
]);

function failure(status: number, code: string): Response {
  return Response.json({ error: { code } }, { status });
}

export function credentialHeaders(headers: Headers): Headers | null {
  const result = new Headers();
  const authorization = headers.get("authorization");
  if (authorization !== null) {
    if (!/^Bearer [!-~]{1,4096}$/.test(authorization)) return null;
    result.set("authorization", authorization);
  } else {
    const cookie = headers.get("cookie");
    if (!cookie || cookie.length > 8192) return null;
    result.set("cookie", cookie);
  }
  return result;
}

export function requestAllowed(request: Request, origin: string): boolean {
  const url = new URL(request.url);
  const requestOrigin = request.headers.get("origin");
  if (requestOrigin !== null && requestOrigin !== origin) return false;
  if (request.method === "GET") return publicGets.has(url.pathname);
  if (request.method !== "POST" || !publicPosts.has(url.pathname)) return false;
  if (browserPosts.has(url.pathname)) return requestOrigin === origin;
  if (url.pathname === "/api/auth/sign-out") {
    return (
      requestOrigin === origin ||
      (requestOrigin === null &&
        request.headers.has("authorization") &&
        credentialHeaders(request.headers) !== null)
    );
  }
  return true;
}

async function boundedBody(request: Request): Promise<Uint8Array | null> {
  const maximum = 16 * 1024;
  const length = request.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > maximum)) return null;
  if (!request.body) return new Uint8Array();
  const reader = (request.body as ReadableStream<Uint8Array>).getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const result = await reader.read();
    if (result.done) break;
    size += result.value.byteLength;
    if (size > maximum) {
      await reader.cancel();
      return null;
    }
    chunks.push(result.value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

async function handle(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname === "/internal/session") {
    if (request.method !== "GET") return failure(405, "method_not_allowed");
    const headers = credentialHeaders(request.headers);
    if (!headers) return failure(401, "sign_in_required");
    const session = await createAuth(env).api.getSession({ headers });
    if (!session) return failure(401, "sign_in_required");
    const operators = new Set(
      (env.OPERATOR_SUBJECT_IDS ?? "")
        .split(",")
        .map((id) => id.trim())
        .filter(Boolean),
    );
    return Response.json({
      id: session.user.id,
      display_name: Array.from(session.user.name).slice(0, 128).join(""),
      is_operator: operators.has(session.user.id),
    });
  }
  if (!requestAllowed(request, env.REGISTRY_ORIGIN)) return failure(403, "request_not_allowed");
  const headers = new Headers(request.headers);
  if (headers.has("authorization")) {
    if (!credentialHeaders(headers)) return failure(401, "sign_in_required");
    headers.delete("cookie");
  }
  let forwarded: Request;
  if (request.method === "POST") {
    const body = await boundedBody(request);
    if (!body) return failure(413, "body_too_large");
    if (url.pathname === "/api/auth/device/code") {
      let data: unknown;
      try {
        data = JSON.parse(new TextDecoder().decode(body));
      } catch {
        return failure(400, "invalid_device_request");
      }
      if (
        !data ||
        typeof data !== "object" ||
        !("client_id" in data) ||
        data.client_id !== "ejm-cli" ||
        ("scope" in data && data.scope !== "registry") ||
        Object.keys(data).some((key) => key !== "client_id" && key !== "scope")
      ) {
        return failure(400, "invalid_device_request");
      }
    }
    forwarded = new Request(request.url, { method: request.method, headers, body });
  } else {
    forwarded = new Request(request, { headers });
  }
  return createAuth(env).handler(forwarded);
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      const response = await handle(request, env);
      const result = new Response(response.body, response);
      result.headers.set("cache-control", "no-store");
      return result;
    } catch {
      return failure(503, "authentication_unavailable");
    }
  },
} satisfies ExportedHandler<Env>;
