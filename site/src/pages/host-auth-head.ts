/** The guide's title and lead, and its section list for the contents column. The text itself lives in HostAuthGuide. */
export const head = {
  slug: "host-auth",
  group: "Rust host guide",
  title: "Host authentication",
  lead: "Keep credentials protected, preserve account identity and change providers between turns.",
};

export const contents = [
  { id: "status", label: "Availability", level: 2 },
  { id: "setup", label: "Key service setup", level: 2 },
  { id: "vault", label: "Vault APIs", level: 2 },
  { id: "migration", label: "Migration and recovery", level: 2 },
  { id: "claude", label: "Claude Code", level: 2 },
  { id: "provider", label: "Provider changes", level: 2 },
  { id: "completion", label: "Run completion", level: 2 },
  { id: "verification", label: "Verification", level: 2 },
] as const;
