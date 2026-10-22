# Proposal

## Why

Users need hundreds or thousands of managed agents to make useful progress without finding concurrency settings by trial and error. The harness has shared resource accounting, but fixed limits cannot respond to host pressure, provider quotas, workload cost, or changes over time.

## What Changes

- Add automatic capacity policy with detected host and container constraints, separate model and local-work targets, and owner-controlled hard ceilings. Keep existing constructors and fixed behavior compatible.
- Coordinate participating processes through one private local host budget, with atomic grants, fair sharing, bounded coordination cost, and monitored recovery after verified owner death. A missed heartbeat never proves death.
- Use native supervision trees on macOS, Windows and Linux. Give controlled helpers private lifetime links, retain launch ownership in bounded memory, and use supported native process scopes. Start the shared execution owner only when needed and exit when idle. Do not require containers, a VM, or a durable per-job execution ledger.
- Prepare shared capacity before package imports or language-host startup, and reuse that public context through final harness construction so startup cannot create a second budget.
- Stage bounded native and package demand in the existing local governor before helper readiness. Preserve queue order while helper and participant grants converge, activate resources atomically, and release unused readiness before waiting so new or reused idle helpers cannot pin all useful capacity.
- Adjust operating targets from completed work, queue wait, service latency, memory and CPU pressure, and structured provider feedback. Probe gradually, reduce promptly, and retain a stable target when extra concurrency brings no useful gain.
- Learn bounded, private local profiles by environment, provider quota domain, and workload class. Age or discard stale evidence; preserve a safe cold start and reset.
- Admit model work before loading full context. Support several thousand managed runs with bounded dispatch, fair service, cancellation, output, storage, and recovery.
- Gate platform activation on one representative 1000-agent, two-process offline workload for useful throughput, responsiveness and shared resource bounds. Reuse relevant safety and boundary evidence; broader scale, hardware and provider matrices remain follow-up coverage. Activate each platform only after its own native ownership and recovery dependencies pass.
- Add public capacity diagnostics and bounded session/event APIs for hosts and SDKs. Update canonical references and public guides when runtime behavior is implemented.

## Capabilities

### New Capabilities

- `adaptive-admission`: Host-aware operating targets beneath immutable safety ceilings, stable feedback control, fair bounded admission, and observable decisions.
- `host-capacity-coordination`: Shared resource reservations across participating processes, bounded contention, identity-checked recovery, and explicit coordination availability.
- `provider-capacity`: Typed provider quota feedback, independent quota domains, and bounded cooldown without unsafe retries.
- `capacity-learning`: Private, bounded, versioned local learning with confidence, expiry, invalidation, and reset.
- `scalable-session-scheduling`: Thousands of lightweight managed runs, admission before expensive preparation, durable bounded event flow, and paged host views.

### Modified Capabilities

None. Existing provider-selection, package-boundary, resource, and recovery requirements remain in force. The documented executor head-of-line policy will change explicitly under `adaptive-admission`.

## Impact

- Owners: `ejm-harness-exec` owns admission, observations, native process ownership, private lifetime links and bounded execution-owner transport; `ejm-harness` owns capacity bootstrap, coarse owner registration and monitored recovery, model scheduling, provider feedback, persistence and session APIs; `ejm-harness-packages` owns governed language-host startup and lifetime accounting through that executor contract. Host adapters dispatch the public private-helper entry point before ordinary CLI or worker startup. Shared runtime behavior stays in the executor. Worker and SDK packages adapt public contracts; `ejm-harness-bench` owns workload evidence.
- Consumers: the paired `ejm-cli` change enables automatic operation and displays capacity through public APIs. Check `ejm-ide` compatibility without adding IDE features or a second scheduler.
- Dependencies flow from hosts and SDKs into the harness. Reuse Tokio, sysinfo, procfs-core, Rustix, rusqlite and private-state APIs. Use standard-library lifetime locks and existing maintained local transport, native process and framing packages before adding dependencies. Remove unused OCI drafts and their dependency cost. Do not build a container runtime or custom OS process parser.
- Confirmed scope: coordinate participating CLI and worker processes on the same host, with local learning and existing tool grants and provider selection. Coordination covers the same OS user and resource domain; other users and applications remain observed external load. Distributed scheduling, remote telemetry, and mandatory background services are outside this proposal.
- Status: implementation in progress. The owner reconsidered mandatory containers and durable execution records, then approved native dependency trees and measured capacity recovery. A confirmed dead owner loses future scheduling authority; possible native survivors become observed external load. This does not certify descendant cleanup or workspace safety on macOS. Existing evidence does not establish automatic tuning, thousand-agent throughput or native support on every platform. No public site publication is part of this change.
