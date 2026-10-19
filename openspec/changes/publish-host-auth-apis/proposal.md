# Proposal

## Why

The CLI's private-relay work depends on credential, official-client and provider-selection APIs that exist only in an unpublished local harness checkout. Publish the smallest coherent dependency change so clean hosted builds can use a reviewed immutable harness revision.

## What Changes

- Add a bounded encrypted credential vault with OS-protected keys, explicit memory-only storage, exclusive transactions and safe provider-state migration.
- Move origin-scoped registry sessions into that vault through an explicit controlled upgrade.
- Add the official Claude Code auth and provider adapters, using bounded private stdin and supervised native child ownership.
- Add optional-provider construction and idle provider replacement with durable continuation binding.
- Include focused offline fixtures and public documentation for these APIs. Keep native evidence gaps explicit.

## Capabilities

### New Capabilities

- `native-credential-storage`: Protected vault persistence, bounded transactions, migration and recovery.
- `registry-credential-storage`: Origin-scoped registry sessions and controlled legacy migration.
- `claude-code-access`: Official-client authentication and bounded inference with harness-owned tools.
- `provider-selection`: Optional provider readiness and durable continuation binding.

### Modified Capabilities

None. The publication base has no main specifications for these APIs.

## Impact

The auth, executor, engine and package crates gain the dependency closure consumed by the CLI. Existing public constructors remain available. Persistent provider and registry storage changes require the documented migration rules. New native credential dependencies require an unlocked OS key service and Linux D-Bus development files to build. The unrelated site rebuild, model catalog, SDK portability and package installation work remain outside this change.

The original API preparation at `943aa862209101624421ca82745cc68642169647` used private standards PR #1 at `b97d5838a68a56495ba17f413a566b7122aabb24`. The prior final-parent candidate used protected standards merge `ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`, whose tree matches reviewed head `6da46394f29b3d0d1f068175b1715e8c6b048d23`. Applying the exact API delta produced reviewed source tree `737cbfec9696eee983681c728ecab8c3bb57e87d` before the publication-reference update.

Prior gate and consumer results remain historical. The final-parent full gate and locked CLI/IDE checks passed as recorded in docs/host-auth-verification.md. The final reviewed publication commit and hosted acceptance remain separate. Publication does not change repository visibility, add Actions credentials, authorize human admission comments or establish release availability.

Hosted run `39302646399` reviewed the later registry-corrected head `86677bbf89f34275614963eb26b91e05fcb7c80b`. AI pass 1 completed with one medium auth cleanup finding; pass 2 completed without findings. Successful Unix auth cleanup retained its numeric process-group identity, allowing a later owner drop to signal an unrelated group after identifier reuse. Retire that identity only after successful group termination and child reaping, before later output-drain waits. Keep it armed through cleanup errors and every pending await, and preserve Windows Job ownership. Add a bounded real-child regression for this retirement boundary. At the retained snapshot, Linux, macOS, registry and standards passed; Windows was still running. None of those results accepts this successor, which needs fresh local and exact-head hosted verification under task 3.5.

Hosted review of publication head `4aeff321c3ab1148bfc92776471552308d3e62da` found that asynchronous vault acquisition could exceed its cancellation and 30-second bounds while the native key service blocked. This follow-up bounds the async caller while keeping key setup serialized and retaining the worker's file lock until an already-started native call returns. The corrected runtime source tree `de80bf8d9a67d54041cb7d167ece5faf55c42ecc` and all four new regressions passed the root-owned required local gate: 202 Rust tests passed with 3 ignored, plus the required SDK and tool checks recorded in docs/host-auth-verification.md. Its 87-input receipt is `752e0b103d746df4e527892062a7728d5113fe6d2f43471bc519034cdfcd0a97`. The trusted caller prerequisite is now adopted at protected commit `99ba4575ad9070a7a0f3296a35e746858af904b0`, tree `c05980717cd691025eebe6f9514b607d40ca01ae`. This preparation merges that commit into API head `4aeff321c3ab1148bfc92776471552308d3e62da` without rewriting its ancestry, then applies the exact ten-path vault correction from frozen tree `4b84fa8790ba8b53c61d0e6270a182b687501bc0`. The production inputs remain identical to the corrected source. A fixture-only follow-up resumes the paused Tokio clock after the deadline jump and before awaiting completion, so the timer and completion guard can progress while the blocked native-key stand-in retains its lock. The resumed-clock source passed its normal local hook and was committed as `90aa7350e30eaad908d14e8b3260610d5f15f2fe`, tree `a2a65b0fd54cd666d25c863c184d2f69a1f6af0b`. Hosted run `39299507686` then found two medium issues in AI pass 2: an ordinary registry integration test required a real OS key service, and opening a legacy FIFO could block before source validation. The retained Linux log confirms the first failure. Refine the existing offline-fixture and safe-migration contract to correct both issues without changing APIs, dependencies, native opt-in coverage or the original vault deadline. The corrected source still needs fresh local and exact-head hosted verification under task 3.5. Real native key-service acceptance remains unproved.
