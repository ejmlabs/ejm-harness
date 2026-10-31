# Proposal

## Why

Native process cleanup is monitored, not guaranteed. Hosts registering the built-in shell currently cannot require an enforced cleanup guarantee before a command runs.

## What Changes

- Allow Rust hosts to require enforced process-tree cancellation for shell calls.
- Expose an opt-in strict process-tree requirement in worker and SDK configuration.
- Preserve the existing registration API and its default behavior.
- Reject a shell call before launch when its host requires an unavailable guarantee.

## Capabilities

### New Capabilities

- `shell-capabilities`: Host-selected process-tree cancellation for shell execution.

### Modified Capabilities

None.

## Impact

The coding crate gains an additive Rust API and the worker and SDK gain an optional configuration field. Existing defaults remain unchanged. No native platform is newly claimed to enforce process-tree cleanup.
