---
name: harness-packages
description: Change ejm-harness package manifests, installation, runtime grants, shared Node or Python hosts, lifecycle adapters, or package SDK integration.
---

# ejm-harness packages

Read [the package contract](../../../docs/packages.md) and the relevant public
types in `ejm-harness-package-spec` and `ejm-harness-packages` before editing.

- Keep manifests and archive validation portable. The native installer and
  Cloudflare registry use the same `ejm-harness-package-spec` contract.
- Pin exact versions and archive hashes. Installation runs no package code or
  lifecycle scripts. New installs and updates start disabled with empty grants.
- Dispatch explicit remote references before filesystem discovery. Download
  public registry metadata and archives through bounded HTTP libraries, with
  fixed origins, no redirects, and verified integrity before persistence. Never
  delegate downloads or extraction to an unbounded package-manager subprocess.
- Native packages run as the current user. Export grants control registration;
  they do not constrain arbitrary Node, Python, filesystem, or network effects.
- Preserve one lazy Node host and one lazy Python host per pool. Bound frames,
  queued bytes, callbacks, packages, imports, output, and cleanup time.
- Use the same host governor for package tools, hooks, commands, and widgets.
  Keep leases and effect ownership until acknowledgement or actual process reap.
  Provider adapters use the engine's model-request admission.
- Reload only while the language host is idle. Keep registered manifests and
  grants fixed. Changing exports or trust requires new registration.
- Hooks may transform authorized inputs and results. They cannot change tool
  identity, engine grants, durable effect evidence, or the final run outcome.
- Use maintained archive, semver, hashing, and transport packages. Keep runtime
  callbacks protocol-only on stdout and drain diagnostics with fixed bounds.

Run archive/manager, both language-host, runtime, and hook fixtures. Update the
worker schema through its generator when wire contracts change. Apply
[harness-registry](../harness-registry/SKILL.md) when publication or registry routes change.
