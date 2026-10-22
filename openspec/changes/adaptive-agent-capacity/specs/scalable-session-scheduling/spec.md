# Spec Delta

## Purpose

Manage thousands of agents with bounded active work and host views while preserving durable ordering, effect ownership, and responsive cancellation.

## ADDED Requirements

### Requirement: Managed count is distinct from runnable work

The automatic runtime SHALL support a documented, measured envelope targeting 100, 1000, and 4096 simultaneously managed agents. It SHALL bound resident waiting state by count and bytes and distinguish managed, runnable, model-waiting, tool-waiting, and executing work. Waiting agents SHALL NOT each require a native process, thread, monitor, or local execution reservation.

#### Scenario: Thousands of agents wait for small active pools

- **WHEN** the supported fixture parks 4096 managed agents while model and local capacity remain small
- **THEN** active work stays within its independent budgets and waiting state stays within the documented memory envelope
- **AND** status does not present the managed count as concurrent model or tool execution

### Requirement: Expensive preparation follows model admission

Model work SHALL obtain memory-aware admission before loading full context or constructing an owned provider request. Context preparation, hooks, provider execution, and completion SHALL retain their required ordering and cancellation behavior. Provider slots SHALL NOT be occupied while a run waits for unrelated local tool execution.

#### Scenario: Many runs wait on a model limit

- **WHEN** only a small number of model requests can start
- **THEN** other waiting runs retain bounded scheduling metadata instead of one fully materialized context per waiter

#### Scenario: Cancellation arrives during preparation

- **WHEN** an admitted run is cancelled before its provider request begins
- **THEN** it releases model and memory credits and sends no later request
- **AND** the durable run outcome remains recoverable

### Requirement: Bursts use bounded backpressure

Accepted managed work SHALL wait through bounded scheduler backpressure when lower-level queues are temporarily full. Queues SHALL bound retained bytes, count, and waiting lifetime. The system SHALL report an explicit reason or terminal budget error rather than losing accepted work, growing queues without bound, or repeatedly failing and retrying admission.

#### Scenario: A thousand agents request tools together

- **WHEN** their accepted demand exceeds the immediate executor waiter capacity
- **THEN** the engine feeds work into that queue within its limits while retaining bounded pending state
- **AND** fitting work progresses fairly as capacity is released

#### Scenario: The accepted-work envelope is exhausted

- **WHEN** a new run cannot fit the managed-work count or byte budget
- **THEN** it receives a clear admission result without starting an effect or silently evicting another run

### Requirement: Durable output remains bounded and ordered

Streaming and journal admission SHALL apply bounded backpressure and preserve per-session durable event order, final content, and effect boundaries. Required events SHALL become visible only after their durability condition is met. Performance changes SHALL NOT weaken synchronous effect intent, uncertain-effect recovery, or serialized mutations within one workspace.

#### Scenario: Storage or a subscriber is slow

- **WHEN** streams produce output faster than journal or presentation consumers can process it
- **THEN** retained queues stay bounded and event order remains recoverable
- **AND** cancellation can stop waiters without replaying an uncertain effect

#### Scenario: Display text writes are batched

- **WHEN** a measured implementation batches compatible output
- **THEN** subscribers receive committed ordered events and the same final text
- **AND** effect and run-transition barriers remain durable before dependent work proceeds

### Requirement: Hosts receive bounded consistent session views

The public API SHALL provide bounded session search and paging, selected-session live events, aggregate status changes, and a consistent presentation snapshot. The snapshot SHALL include authoritative current run identity, last run-state transition, durable cursor, and a bounded transcript tail with an omitted-history indicator. Full durable history SHALL remain separately pageable.

#### Scenario: A host opens a long-lived session

- **WHEN** it requests current state and a bounded presentation tail
- **THEN** it can render and validate run-bound actions without replaying all prior events
- **AND** the snapshot's state and cursor describe one consistent durable boundary

#### Scenario: Output races snapshot and subscription

- **WHEN** events arrive while a host subscribes and obtains its snapshot
- **THEN** the documented cursor handshake permits ordered deduplication and replay without gaps
- **AND** lag recovery uses bounded pages or a fresh consistent snapshot

#### Scenario: Session search crosses page boundaries

- **WHEN** a host pages or filters thousands of sessions
- **THEN** it receives bounded results with stable identities and deterministic continuation semantics

### Requirement: Cancellation and shutdown release owned work

Queued, preparing, and active work SHALL retain prompt cancellation and complete accounting through dropped futures, spawn failures, and cleanup. Graceful shutdown SHALL stop submissions, drain local ownership, and release shared grants only after drain. Raising managed capacity SHALL NOT weaken existing per-run control, token, output, process, and artifact bounds.

#### Scenario: A large queued workload is cancelled

- **WHEN** many waiters are cancelled or their handles are dropped according to the existing contract
- **THEN** all owned credits and queue records are removed through bounded cleanup
- **AND** no cancelled request starts later or leaves a shared allocation incorrectly released

### Requirement: Selected-stream continuity is explicit

Selected-session subscriptions SHALL expose continuity through a documented predecessor token or explicit channel-loss signal. Snapshot and replay SHALL provide the matching continuity anchor. A gap between global event sequence numbers SHALL NOT by itself imply a lost selected-session event, because other sessions share that sequence space.

#### Scenario: Other sessions interleave durable events

- **WHEN** two consecutive selected-session events have global sequence numbers separated by events from other sessions
- **THEN** their valid selected-stream continuity is preserved without unnecessary gap recovery
- **AND** actual selected-channel loss still triggers bounded recovery

### Requirement: Legacy worker omissions preserve capacity behavior

Existing worker requests that omit capacity configuration SHALL retain legacy fixed semantics. New SDK defaults SHALL explicitly request automatic coordinated policy through an additive field. Decoding compatibility SHALL NOT silently change an old request's admission mode, and nonparticipating legacy work SHALL be identified as external load by coordinated clients.

#### Scenario: An older SDK starts the new worker

- **WHEN** its request contains no capacity policy field
- **THEN** it retains the legacy fixed policy rather than silently joining automatic coordination
- **AND** a new SDK's explicit automatic request joins the shared host scope through the same core contracts

### Requirement: Representative activation evidence

A platform's automatic default SHALL require relevant native recovery evidence and one representative offline 1000-agent workload across two participating processes. Existing evidence MAY satisfy unchanged runtime and dependency paths. Unmet critical gates SHALL block only that platform, and unmeasured broader targets SHALL NOT be claimed passed.

#### Scenario: Reuse unchanged safety and helper evidence

- **WHEN** a recorded source comparison establishes unchanged exercised runtime and dependency paths
- **THEN** relevant accounting, recovery and helper calibration evidence may be reused
- **AND** historical samples retain their original image labels even if unrelated linked code changes the current executable hash
- **AND** only affected or uncovered risks require new runs

#### Scenario: Bound the representative comparison

- **WHEN** the representative workload compares automatic capacity with safe fixed capacity and active terminal interaction
- **THEN** it uses a three-point fixed sweep and two alternating paired trials, with at most one additional pair for noise
- **AND** its critical tolerances remain 10% for automatic useful throughput, 50 ms for consumer input/Stop/event-to-paint p95 and 5% for active-terminal overhead
- **AND** an unresolved result triggers investigation of its concrete cause rather than a wider matrix

#### Scenario: Broader coverage remains follow-up work

- **WHEN** the platform's critical native recovery and representative-workload gates pass
- **THEN** full scale, hardware, provider, package-language and precision profiling matrices do not block activation without a concrete uncovered risk
- **AND** existing focused 4096-run admission/cancellation evidence remains relevant
- **AND** another platform's unresolved native ownership or cgroup-move dependency does not establish support or block the verified platform
