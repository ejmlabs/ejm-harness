# Capacity release status

This branch is prepared for review. It is not a product release.

## Verified locally

- Exact public filesystem fork commit: `ad6faff2442ffe6abccd1856f4c0df9a3e472035`.
- Harness and CLI source allowlists accept only that Git repository. All three consumer lockfiles resolve it.
- Current-protocol registry open succeeds under a WAL writer; incompatible protocol and replaced database rejection pass.
- Clean helper startup exit permits one retry; nonzero exit fails immediately; ownership release passes.
- The scoped release integration passed strict CLI Clippy, release builds, all four Stop regressions, fixed/automatic shared-root smoke and cleanup, and all 19 retained relay backend contracts after the helper-retirement repair. See `capacity-integration-checkpoint.json` for receipts.

## Remaining release gates

- Finish representative workload acceptance after the helper startup repair. The last automatic case completed 6,777 useful runs but failed one run before helper readiness; cleanup passed. Automatic defaults remain disabled.
- The final representative workload is blocked before startup by 4.4 GiB free disk, below the 10 GiB guard. No workload ran in that attempt. Source manifests still matched the passed checkpoint.
- Obtain required CI and human review, merge through protected main, then have a current human maintainer run the source-release workflow.
- Cargo registry credentials are not required for GitHub source builds. Registry publication, signed native artifacts and other platforms remain separate.

The integration preserves current-main authentication and relay safety fixes. Unrelated credential-vault optimization, browser relay work and site redesign are excluded. Original benchmark reports and compressed raw evidence remain available locally with restoration receipts.
