# Proposal

## Why

Two executor tests depend on the operating system scheduling a fixture before a deadline or monitor cancellation. Either can fail before the fixture publishes a PID file even when the behavior under test works.

## What Changes

- Synchronize the deadline test with fixture readiness before advancing a controlled test clock.
- Publish the root and one descendant before exceeding the process budget, so their cleanup remains independently verifiable after cancellation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. The change only stabilizes tests and does not alter the executor contract.

## Impact

Executor integration tests and the Tokio test-only feature; no production API or runtime behavior changes.
