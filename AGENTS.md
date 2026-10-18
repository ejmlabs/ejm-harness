# ejm-harness

Reusable open source Rust engine, coding tools, executor, worker, and language
SDKs. ejm-cli lives in its own sibling repository. No Gerry Workspace dependency.

- Never use em dashes. Use plain, direct prose.
- Read [the contributor contract](docs/contributing.md) before changing architecture or quality policy.
- Keep Rust ownership and admission in the harness. SDKs adapt the same public worker protocol.
- Use one shared governor for every tool, command, and child agent in a harness instance.
- Bound queues, output, context, retries, steps, artifacts, and process lifetimes.
- Never replay an uncertain effect after recovery.
- Report native restrictions as enforced, monitored, or unsupported. Native execution is not a sandbox.
- Public traits are dependency injection ports. Add a trait where an external boundary or real variation needs one.
- Inherit workspace lints. Do not relax groups or add production panic shortcuts.
- Use trusted, maintained packages for standard behavior. Write custom code
  for ejm-harness's distinct behavior, such as admission, recovery, and context
  policy. Document any exception with the package alternatives and evidence.
  Do not handroll protocol parsing, cryptography, or test servers.
- Run `./scripts/check` before handing off completed work. Use focused checks while editing.
- Keep Cargo jobs at most two on constrained machines.
- Use replay or local HTTP fixtures. Tests must not call paid models.
- Do not publish packages, upload artifacts, or deploy without explicit authorization.
- If `openspec/` is added, follow the installed OpenSpec workflow for feature and bugfix work. Do not initialize it without a request.

## Project skills

Load the skill for the part of the system being changed. Load more than one only
when the change crosses those boundaries.

- [harness-engine](.agents/skills/harness-engine/SKILL.md): driver, providers, policies, history, and recovery.
- [harness-auth](.agents/skills/harness-auth/SKILL.md): provider sign-in, credential storage, identity, renewal, and logout.
- [harness-execution](.agents/skills/harness-execution/SKILL.md): admission, command supervision, output, and native guarantees.
- [harness-sdk](.agents/skills/harness-sdk/SKILL.md): worker protocol, generated types, and TypeScript or Python facades.
- [harness-performance](.agents/skills/harness-performance/SKILL.md): latency, memory, concurrency, and measured claims.
- [harness-maintenance](.agents/skills/harness-maintenance/SKILL.md): architecture decisions, strict gates, and updating these rules from evidence.
- [harness-packages](.agents/skills/harness-packages/SKILL.md): portable manifests, installation, grants, language hosts, and extension adapters.
- [harness-registry](.agents/skills/harness-registry/SKILL.md): Cloudflare registry, publisher accounts, immutable versions, login, moderation, and security checks.
