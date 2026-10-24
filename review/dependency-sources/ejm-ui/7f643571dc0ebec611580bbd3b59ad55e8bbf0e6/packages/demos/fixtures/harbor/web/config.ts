export interface Config {
  host: string;
  port: number;
}

// Read the service config from the environment.
export function loadConfig(env: Record<string, string | undefined>): Config {
  const host = env.HARBOR_HOST ?? "127.0.0.1";
  const port = Number(env.HARBOR_PORT ?? "8080");
  return { host, port };
}
