# Spec Delta

## Purpose

Adapt model admission to provider capacity and quotas without confusing remote limits with local resources or repeating uncertain effects.

## ADDED Requirements

### Requirement: Provider feedback is typed and redacted

Built-in providers SHALL expose bounded structured capacity observations for throttling, supported quota hints, cooldowns, request timing, and reported usage. Scheduling SHALL NOT depend on parsing human-readable error strings or enabling HTTP debug traces. Providers lacking such observations SHALL remain usable with explicitly unknown quota information.

#### Scenario: A provider rejects excess requests

- **WHEN** a local HTTP fixture returns a supported throttle response and retry hint
- **THEN** the scheduler receives a typed throttle and bounded validated hint
- **AND** credentials, raw headers, response bodies, and request text do not enter learned capacity state

#### Scenario: A custom provider supplies no quota hints

- **WHEN** the host uses an existing provider implementation
- **THEN** model requests still run through compatible public contracts
- **AND** capacity status reports unknown quota information without fabricating a rate limit

### Requirement: Model admission respects distinct quota domains

Model admission SHALL separate transport and quota domains using nonsecret identity and provider-declared sharing rules. Participating processes with the same quota identity SHALL share applicable host-local quota credits. A throttle SHALL NOT reduce unrelated domains, and different model names SHALL NOT imply independent quotas when the provider declares shared limits.

#### Scenario: Two processes share an account quota

- **WHEN** both identify the same supported provider quota domain
- **THEN** their applicable in-flight and rate allocations charge that shared domain
- **AND** opening a second process cannot independently spend the whole quota

#### Scenario: One account is throttled

- **WHEN** one quota domain enters cooldown while another is healthy
- **THEN** the healthy domain can continue within local and shared host budgets

### Requirement: Rate budgets and concurrency are separate

Where supported or explicitly configured, admission SHALL respect request and token rate budgets as well as in-flight limits. Unknown response usage SHALL retain a conservative reservation until safely reconciled. Provider cooldown SHALL govern future admissions, with bounded re-entry that avoids synchronized bursts across participating processes.

#### Scenario: Requests are short but rate-limited

- **WHEN** completed requests free model slots before the request-rate budget replenishes
- **THEN** new requests wait for rate capacity despite available concurrency

#### Scenario: A cooldown ends with many waiting agents

- **WHEN** participating processes have queued demand after the same retry deadline
- **THEN** bounded coordinated re-entry prevents all waiters from starting at once
- **AND** malformed or unsupported retry hints cannot cause a busy loop or an earlier-than-required retry

### Requirement: Capacity control does not replay uncertain work

Capacity feedback SHALL control future admission without automatically replaying failed or interrupted work whose execution is uncertain. Cancellation and run deadlines SHALL remain effective during quota waits. Context, token, tool-grant, and effect-recovery contracts SHALL remain unchanged.

#### Scenario: A connection fails after the request was sent

- **WHEN** the provider outcome is uncertain
- **THEN** the scheduler records the capacity outcome without automatically repeating that model request or any proposed tool effect

#### Scenario: A queued run is cancelled during cooldown

- **WHEN** cancellation or the run deadline occurs before quota admission
- **THEN** the waiter leaves promptly, releases every reserved credit, and performs no later request

### Requirement: Provider charges survive local ownership changes

Provider request/token charges SHALL remain valid until their applicable provider window or reset even when a process exits or the host reboots. Local owner expiry SHALL NOT refund uncertain remote usage. In-flight guarantees SHALL describe locally tracked streams unless the provider supplies stronger execution-lifetime evidence.

#### Scenario: The host reboots during remote inference

- **WHEN** local owner records become stale while allocated provider-rate charges have not expired
- **THEN** restart preserves those charges until their actual window/reset and does not replay the uncertain request
- **AND** status does not claim that local process death proved remote inference ended
