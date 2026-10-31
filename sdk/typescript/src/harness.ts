import { HarnessError, asError } from "./errors.js";
import type {
  ConfigureParams,
  Event,
  GovernorSnapshot,
  HarnessSnapshot,
  HelloResult,
  PackageInfo,
  PackagePage,
  Session as SessionInfo,
} from "./protocol.js";
import { Session } from "./session.js";
import { Transport, type TransportOptions } from "./transport.js";

const PROTOCOL_VERSION = 1;
const MAX_PACKAGES = 128;
const PACKAGE_PAGE_SIZE = 16;

export type HarnessOptions = Omit<ConfigureParams, "tools"> & TransportOptions;

function packageCall(
  packageName: string,
  name: string,
  sessionId: string,
  args: unknown,
): { package: string; name: string; session_id: string; arguments: unknown } {
  return { package: packageName, name, session_id: sessionId, arguments: args };
}

/** One persistent Rust worker, one shared governor, many sessions. */
export class Harness extends Transport {
  private constructor(options: HarnessOptions) {
    super(options);
  }

  static async create(options: HarnessOptions = {}): Promise<Harness> {
    const harness = new Harness(options);
    try {
      await harness.handshake(options);
      const config = { ...options };
      delete config.workerPath;
      delete config.requestTimeoutMs;
      await harness.request("configure", {
        ...config,
        tools: (options.tools ?? []).map((tool) => tool.definition),
      });
      return harness;
    } catch (error) {
      await harness.abort(asError(error));
      throw error;
    }
  }

  async createSession(options: { cwd?: string; title?: string } = {}): Promise<Session> {
    const info = await this.request<SessionInfo>("session.create", {
      cwd: options.cwd ?? process.cwd(),
      title: options.title ?? "",
    });
    return new Session(this, info);
  }

  async listSessions(): Promise<Session[]> {
    return (await this.request<SessionInfo[]>("session.list", {})).map(
      (info) => new Session(this, info),
    );
  }

  async governor(): Promise<GovernorSnapshot> {
    return this.request("governor.status", {});
  }
  async status(): Promise<HarnessSnapshot> {
    return this.request("harness.status", {});
  }

  async listPackages(): Promise<PackageInfo[]> {
    const packages: PackageInfo[] = [];
    let after = 0;
    for (;;) {
      const page = await this.request<PackagePage>("package.list", {
        after,
        limit: PACKAGE_PAGE_SIZE,
      });
      packages.push(...page.packages);
      if (packages.length > MAX_PACKAGES)
        throw new HarnessError("protocol", "package list exceeds 128 entries");
      if (page.next == null) return packages;
      if (page.next <= after || page.next > MAX_PACKAGES)
        throw new HarnessError("protocol", "invalid package page cursor");
      after = page.next;
    }
  }

  /** Reload idle runtime code while keeping the configured manifest and grants. */
  async reloadPackage(packageName: string): Promise<void> {
    await this.request("package.reload", { package: packageName });
  }

  async invokePackageCommand(
    packageName: string,
    name: string,
    sessionId: string,
    args: unknown = {},
  ): Promise<unknown> {
    return this.request("package.command", packageCall(packageName, name, sessionId, args));
  }

  /** Return widget data for the host to render. */
  async renderPackageWidget(
    packageName: string,
    name: string,
    sessionId: string,
    args: unknown = {},
  ): Promise<unknown> {
    return this.request("package.widget", packageCall(packageName, name, sessionId, args));
  }

  async events(sessionId: string, after = 0, limit = 64): Promise<Event[]> {
    return this.request("events.list", { session_id: sessionId, after, limit });
  }

  /** Check the worker speaks this protocol and supports what the configuration needs. */
  private async handshake(options: HarnessOptions): Promise<void> {
    const hello = await this.request<HelloResult>("hello", { protocol: PROTOCOL_VERSION });
    if (hello.protocol !== PROTOCOL_VERSION)
      throw new HarnessError("protocol", "worker protocol 1 is required");
    if (
      options.require_shell_process_tree_cancellation &&
      (!Array.isArray(hello.capabilities) ||
        !hello.capabilities.includes("shell_process_tree_cancellation"))
    ) {
      throw new HarnessError(
        "protocol",
        "update ejm-harness-worker to a version that supports strict shell process-tree cancellation",
      );
    }
    if (
      ((options.packages?.length ?? 0) > 0 || options.provider?.kind === "package") &&
      (!Array.isArray(hello.capabilities) || !hello.capabilities.includes("packages"))
    ) {
      throw new HarnessError(
        "protocol",
        "update ejm-harness-worker to a version that supports packages",
      );
    }
  }
}
