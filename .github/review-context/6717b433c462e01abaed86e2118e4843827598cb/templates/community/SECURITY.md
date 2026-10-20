# Security policy

## Supported versions

The latest published patch of each documented supported minor line receives security fixes. Before a stable release, only the latest prerelease and current main are maintained. See [releases](https://github.com/{{repository}}/releases) for actual published versions. Source availability does not establish production readiness.

## Report a vulnerability privately

Use [GitHub private vulnerability reporting](https://github.com/{{repository}}/security/advisories/new). Do not put exploit details, credentials, or personal data in public issues or PRs. If this channel is unavailable, contact a repository maintainer through the contact method on their GitHub profile and ask for a private channel before sending details.

Include the affected revision/version, scope, prerequisites, reproducible steps, impact, and a suggested fix if you have one. Use synthetic data. We aim to acknowledge a report within five business days and agree on a fix and disclosure plan; this is a target, not a guaranteed service level. We do not offer a paid bounty unless one is separately announced.

## Review priorities

Review credential handling, authorization and approval boundaries, workspace path/symlink isolation, untrusted package and repository content, command execution, network input, dependency changes, and CI/release permissions. Test realistic attack paths. Do not infer safety from a passing scanner. Maintainers must review public history and release artifacts for private data before changing visibility.

Use minimal workflow permissions and immutable action pins. Never execute PR code with model or release credentials. Each PR head needs a human readiness decision before checks or AI run, followed by current maintainer approval and passing required checks. See [Contributing](CONTRIBUTING.md).
