# Design

## Context

At the start of this audit, the projects were private and main had no branch protection. Product workflows ran on every PR with failing remote runs, and the two hosts required a sibling harness checkout. All products currently use version 0.1.0. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:** Share maintenance policy and trusted automation, keep each product independently versioned, and require a human to admit and approve the exact PR head.

**Non-Goals:** Publishing product releases, package uploads, installer signing, deployment, history rewrites, and buying private-repository security products during setup.

## Decisions

1. Use the user-selected ejmlabs/.github repository for reusable workflows, executable standards, templates, and runbooks. Consumers pin a reviewed full SHA. Product checks keep their established gate commands.
2. Use trusted default-branch issue-comment or manual dispatch entry points. A human maintainer issues `/ready <full-head-sha>` before quality or AI runs. New commits have no inherited readiness. Outsider and bot events do not launch jobs. Restrict GitHub fork workflows to approval by all external contributors and keep untrusted jobs read-only.
3. Keep readiness separate from `/approve <full-head-sha>`. The latter requires a current human maintainer and records native bot approval for self review where GitHub forbids native author approval. Main requires at least one native approval and the maintainer gate, plus required checks, resolved discussions, linear history, and no force pushes or deletion. Administrators do not bypass the merge requirements.
4. Run exactly two independent Codex passes using `gpt-6.1-sol` with medium reasoning, trusted prompts and configuration, read-only permissions, no PR build or test execution, and isolated model credentials. Post results in a separate job. Missing credentials or invalid output fail closed. Bound the diff, output, time, and concurrent runs. Check replayed admission identifiers before spending budget.
5. Keep a root VERSION and CHANGELOG.md in each product. Validate all releasable package versions. A common manual pipeline accepts an exact checked main commit and matching version, refuses existing tags, and creates only a draft release with source archives, checksums, and provenance. Document native binary and registry publication gates explicitly.
6. Keep licenses and brand notices distinct. Preserve each product's MIT license and bundled font licenses; include license text in SDK packages. The studio's design assets retain their existing ownership terms.
7. Enable available free repository protections and dependency alerts now. Do not enroll private repos into billed secret protection. Record the required public-only activation checks before any visibility change.

## Risks / Trade-offs

- Shared workflow changes affect several products. Immutable caller pins and reviewed update PRs bound rollout.
- Human readiness costs a small metadata action and can delay outside contributions. The expensive checks remain unavailable until deliberate admission.
- CLI and IDE quality checks remain blocked until the harness is public. The owner chose not to provision a private cross-repository token. After the harness fixes merge, update both hosts to that reviewed commit and rerun their required gates.
- API review needs a scoped OPENAI_API_KEY and provider budget settings. Setup does not prove a successful paid review.
- Local disk space is constrained. Coordinate Cargo checks, use focused existing caches, and report missing full platform evidence honestly.
- Source and history scans reduce known risk; partial scan coverage and platform-specific tests remain explicit.

## Migration Plan

Create and test the shared repository first. Add pinned consumers and version metadata, then apply and read back repository settings. Run the project gates and first admitted PR through the new pipeline when the required credentials are present. Keep repositories private until blockers are resolved. Roll back a bad workflow update by restoring the previous immutable caller pin.

## Harness adapter

Use ejm-cli consistently in device code, polling, consent UI, and real auth fixtures. Add request-body and unregistered-client regressions. Run the registry gate in an isolated CI job alongside the workspace gate. Apply protected path checks to reads and listings before returning data, including canonical symlink aliases. Include MIT text and repository metadata in language SDK distributions.
