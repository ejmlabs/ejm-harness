# Tasks

Checked tasks record the original isolated API preparation at `943aa862209101624421ca82745cc68642169647`, based on `b97d5838a68a56495ba17f413a566b7122aabb24`. The prior final-parent candidate used protected parent `ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`; its exact API reapplication matched reviewed tree `737cbfec9696eee983681c728ecab8c3bb57e87d` before the publication-reference update. The current preparation preserves API head `4aeff321c3ab1148bfc92776471552308d3e62da`, merges protected caller `99ba4575ad9070a7a0f3296a35e746858af904b0` and applies the frozen vault correction. The preparation first matched all 87 runtime inputs of corrected tree `4b84fa8790ba8b53c61d0e6270a182b687501bc0`. A later fixture-only correction resumes the clock before the native-key deadline test awaits completion; the other 86 inputs remain identical. The resumed-clock source passed its normal local hook and was committed as `90aa7350e30eaad908d14e8b3260610d5f15f2fe`, tree `a2a65b0fd54cd666d25c863c184d2f69a1f6af0b`. Hosted run `39299507686` reviewed that head: pass 1 completed without findings; pass 2 completed with two medium findings. Its Linux gate failed the obsolete registry integration test because no usable OS key service was available, while the equivalent injected-key test passed. The other finding is a legacy FIFO open that can block before descriptor validation. These results require the correction and fresh verification below; they do not complete hosted acceptance. Earlier results in docs/host-auth-verification.md remain bound to their named snapshots.

## 1. Protected credential APIs

- [x] 1.1 Extract the shared vault, native protected-file adapters, provider compatibility entry point and exact direct dependencies; include the existing tampering, bounds, locking, migration, recovery and injected-key fixtures and canonical/public guidance, then verify the focused auth suite and strict lint checks.
- [x] 1.2 Extract origin-scoped registry vault storage and controlled migration, including source privacy and permanent markers; include the focused stale-source, wrong-origin, interruption and confirmed-handoff fixtures and docs, then verify the package suite.

## 2. Official-client and provider APIs

- [x] 2.1 Extract supervised private stdin and native child ownership with existing output, cancellation, EOF and abandoned-caller fixtures; update the executor contract and verify its focused suite.
- [x] 2.2 Extract official Claude Code authentication and structured provider access with its maintained discovery dependencies, offline fixtures and public setup/limit guidance; verify the auth and provider suites without sign-in or paid inference.
- [x] 2.3 Extract optional-provider construction and idle provider replacement with only the durable binding journal changes; include the deterministic readiness, continuation, restart and failure-atomicity fixtures and docs, then verify the focused provider-selection suite.

## 3. Integration and review

- [x] 3.1 Resolve and commit the scoped Cargo lock through the parent-owned build gate; run the full required harness gate and locked CLI/IDE consumer checks, and record actual platform, key-service and performance evidence without claiming unavailable acceptance.
- [x] 3.2 Check public documentation, links, formatting and strict OpenSpec validation; review the exact diff and measure its file, line and byte totals below central admission limits before private PR publication.
- [x] 3.3 Run the required full harness gate and locked CLI/IDE consumer checks on the final-parent candidate; refresh exact source receipts, scope and verification records, retaining prior results as historical evidence and leaving hosted and native acceptance limits explicit.
- [x] 3.4 Bound asynchronous native-key waits by cancellation and the original deadline while retaining worker ownership; add deterministic blocked-key regressions and update the source and public recovery contracts.
- [ ] 3.5 Correct the hosted registry and auth process-group findings, preserve the resumed-clock native-key deadline regression, then run focused auth/package checks and the required full gate on the corrected candidate and obtain fresh exact-head hosted acceptance using the adopted protected caller. Keep prior results as historical evidence.

    Remove only the obsolete OS-store-dependent registry integration case after confirming its assertions remain covered by the existing injected-key test. Preserve explicitly opt-in native key-service coverage. Reject Unix legacy FIFOs before a blocking read with the existing maintained nonblocking open and descriptor validation; add a bounded Linux regression that joins its watchdog and verifies source and destination preservation. Keep the original 30-second vault deadline, cancellation, ownership and recovery assertions unchanged. Do not count Linux, macOS, Windows or registry checks as passed before their actual completed results.

    Hosted run `39302646399` on registry-corrected head `86677bbf89f34275614963eb26b91e05fcb7c80b` adds one medium auth cleanup finding. Retire the Unix numeric process-group identity only after successful group termination and child reaping, with no intervening await before return. Preserve cleanup ownership on errors or abandoned waits, all Windows Job behavior and existing cancellation/descendant coverage. Add a bounded real-child regression that detects the retained identity without trying to force identifier reuse. Linux, macOS, registry and standards passed on that historical head; Windows was still running at the retained snapshot. Fresh successor acceptance remains required.

## Workflow follow-up

- Review the final snapshot with the parent before commit or push. Retain the protected standards parent and all human admission and visibility controls.
- After verification and reviewed merge, sync these requirements and archive the change through the OpenSpec workflow. Keep broader cross-platform evidence in its owning change.
- Update the CLI's immutable harness pin only after its required APIs are present in the reviewed dependency revision.
