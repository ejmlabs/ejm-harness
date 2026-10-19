# Spec delta

## Purpose

Let native hosts observe the completion of one exact run without copying the engine's admission or lifecycle state.

## ADDED Requirements

### Requirement: Exact current run subscription

The engine SHALL offer a completion handle only for the named session's exact current run, including a run started by its internal queue. A missing or superseded run SHALL return no handle and SHALL NOT imply successful completion.

#### Scenario: A queued successor becomes current

- **WHEN** an internally queued successor starts and the host names its session and run
- **THEN** the engine supplies that successor's handle and rejects a different session, the predecessor or a different run

### Requirement: Shared completion barrier

Handles for the same run SHALL share its completion and cancellation state. Waiting SHALL include durable completion and bounded run-end observers. Dropping a handle SHALL NOT cancel the run. A driver that ends without a summary SHALL remain an error.

#### Scenario: Completion is durable before observers finish

- **WHEN** a run-finished event is durable while a run-end observer is still blocked
- **THEN** every completion waiter remains pending until that observer returns or reaches its existing deadline, and the waiters receive the same run result

#### Scenario: One observer leaves

- **WHEN** one cloned handle is dropped while another still waits
- **THEN** the run continues and the remaining observer receives the run result

### Requirement: Atomic queued-successor decision

Detailed completion SHALL include the summary and whether the driver handed off to an internally queued successor. The engine SHALL decide that flag after draining late steering and applying continuation rules under active admission ownership, before notifying completion observers. Summary-only wait SHALL preserve its existing result. A false flag SHALL NOT assert current idleness or authorize a later host action.

#### Scenario: An internal successor finishes before observation

- **WHEN** a completed run hands off to a local queued successor that finishes before the host observes the predecessor's completion
- **THEN** the predecessor's detailed result still reports the handoff and remains equivalent to the legacy summary-only wait

#### Scenario: No local successor is admitted

- **WHEN** the driver completes its final run without handing off to another local prompt
- **THEN** detailed completion reports no queued successor while the host retains its own admission and target checks
