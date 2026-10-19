# Design

## Context

See proposal.md for the publication need. The private base contains the earlier plaintext provider and registry stores, one executor supervisor and a fixed engine provider. The active local source already implements the required APIs under approved credential, Claude and runtime-provider plans. This change extracts their coherent dependency closure into a clean checkout instead of publishing that checkout's unrelated work.

## Goals / Non-Goals

**Goals:** Keep one credential implementation, one governor, bounded native child supervision and idle provider configuration. Preserve public constructors and the existing wire protocol. Make every required source, fixture and documentation change reviewable below central PR limits.

**Non-Goals:** Native platform support certification, site redesign, model catalogs, SDK portability, package installation changes, public visibility, release packaging or deployment.

## Decisions

- Base the final candidate on protected standards merge `ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`, whose tree matches reviewed PR #1 head `6da46394f29b3d0d1f068175b1715e8c6b048d23`. Reapply only the original API delta from `b97d5838a68a56495ba17f413a566b7122aabb24` to `943aa862209101624421ca82745cc68642169647`. Never bypass current human admission or inject a personal token into Actions.
- Reuse the existing AWS-LC AEAD dependency for a bounded authenticated envelope and the maintained keyring-core native stores for its key. The encrypted payload is capped before decoding. The protected-file adapter uses safe Rustix on Unix and RightKit on Windows/macOS. Keep only thin path, transaction, migration and namespace policy in project code. See docs/credential-vault.md for package fit, budgets and recovery.
- Retain the provider store's stable legacy lock and publish an unsupported-version barrier only after verified encrypted commit. Keep registry imports explicit because old registry writers do not share that lock. Destination markers survive deletion and prevent stale reimport.
- Reuse the official unmodified Claude Code client. Auth stays independent of execution and terminal code. The provider uses the shared executor with bounded private stdin, clears ambient auth/routing overrides and rechecks subscription status before every inference. Native tools and ordinary customizations are disabled while managed provider policy remains effective.
- Keep child ownership in a small executor adapter. Unix process groups and maintained Windows Job ownership retain cleanup through pipe drain and lease release. Do not include unrelated package-host or SDK process changes.
- Persist provider binding and continuation removal in one SQLite transaction under admission ownership. Extract only that journal hunk, preserving the base journal implementation and excluding native SQLite file-hardening work.
- Update the base static public documentation directly. Do not copy the concurrent React/Vite site migration. Document source availability and unverified native acceptance explicitly.

## Risks / Trade-offs

- OS key service access can block or fail. Hosts call synchronous key boundaries off event threads; errors preserve encrypted state and explicit memory-only selection never happens silently.
- Encryption and native key dependencies add build and runtime cost. The existing local measurements are prior-checkout evidence, not proof for this extraction. Repeat the documented credential workload before acceptance.
- Claude launches a process per check and inference and returns a final text delta. Offline fixtures do not prove live subscription behavior or native Windows cleanup.
- The standards prerequisite is merged at protected `ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`. The final-parent API gate and locked CLI/IDE checks passed. Hosted acceptance and the reviewed publication commit remain separate. The final PR base and immutable dependency pin must identify the admitted source revision, and each submitted diff must remain below 200 files, 12,000 changed lines and 1 MB.
- Trimmed dependency scope requires a newly resolved Cargo lock and clean-source harness and CLI checks. Do not present earlier broad-checkout results as this snapshot's gate.

## Migration Plan

1. Extract the required source, meaningful fixtures, canonical/public docs and exact direct dependency pins.
2. Regenerate the lock through the parent-owned Cargo gate. Run focused auth, executor, package and provider fixtures, then the required harness gate and consumer checks.
3. Review the exact snapshot, publish a private draft PR only after the parent confirms it, and retain normal human admission. Keep repository visibility and Actions secrets unchanged.
4. After reviewed dependency merge, update the CLI's immutable harness revision and verify its source extraction in hosted CI. Native evidence and dedicated OAuth setup remain separate tracked prerequisites.
5. A rollback must preserve ciphertext and migration barriers. Never export credentials to plaintext or retry an old rotating token.
