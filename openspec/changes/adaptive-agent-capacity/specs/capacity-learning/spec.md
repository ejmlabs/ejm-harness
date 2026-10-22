# Spec Delta

## Purpose

Use bounded private evidence from ordinary work to improve startup and adaptation without turning past capacity into permission to exceed current limits.

## ADDED Requirements

### Requirement: Local profiles contain aggregate capacity evidence

Capacity learning SHALL retain only bounded nonsecret aggregate observations needed to compare workload cost, throughput, latency, and sustainable targets. Profiles SHALL be scoped by controller version, relevant environment and resource domain, workload class, and provider quota identity. Prompts, command text, paths, credentials, and raw HTTP traces SHALL NOT be stored in profiles.

#### Scenario: Ordinary work produces learning evidence

- **WHEN** representative model and tool operations complete
- **THEN** the profile can record counts, timing distributions, resource estimates, observed pressure, and confidence within its fixed bounds
- **AND** it cannot reconstruct the work's content or secrets

### Requirement: Learned targets are revalidated startup hints

A learned target SHALL be revalidated against current hard ceilings, shared host grants, pressure, and observation freshness before use. Incompatible or materially changed environments SHALL lower confidence or invalidate the profile. Missing, corrupt, unsupported, or expired profiles SHALL fall back to safe cold-start behavior without blocking ordinary startup.

#### Scenario: A saved profile comes from a larger resource envelope

- **WHEN** the process starts with tighter memory or CPU constraints
- **THEN** cached capacity cannot bypass those constraints or start with an unvalidated larger grant

#### Scenario: A profile is corrupt

- **WHEN** the bounded profile cannot be decoded or validated
- **THEN** the controller ignores it, reports that cold-start learning is in use, and leaves durable session history intact

### Requirement: Learning decays and remains bounded

Profile count, serialized bytes, sample state, and write frequency SHALL have documented limits. Old evidence SHALL decay or expire, and profile eviction SHALL preserve correct execution without a full history scan. A learning write failure SHALL NOT weaken shared reservation accounting, durable effect boundaries, or cancellation.

#### Scenario: Many provider and workload combinations are observed

- **WHEN** the profile count or byte budget is reached
- **THEN** the learner evicts or compacts bounded aggregate state rather than growing without limit
- **AND** current grants and safety ceilings remain unchanged

#### Scenario: Learning persistence is unavailable

- **WHEN** a profile update cannot be saved
- **THEN** valid work can continue under existing capacity policy and coordination
- **AND** status distinguishes unavailable learning from unavailable shared admission

### Requirement: Owners can reset learning without losing work

The public API SHALL allow an authorized local host to inspect learning status and reset the relevant learned profiles. Reset SHALL NOT erase sessions, credentials, grants, artifacts, or active reservation accounting. Agents and remote controllers SHALL NOT acquire reset or capacity-setting authority through generated content.

#### Scenario: The local user resets capacity learning

- **WHEN** reset succeeds during or between runs
- **THEN** future tuning starts from conservative evidence for the selected scope
- **AND** active work retains its leases, output, history, and current hard limits
