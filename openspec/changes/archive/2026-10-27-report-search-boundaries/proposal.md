# Proposal

## Why

Native search can omit deeper files or part of the last file while claiming the scan is complete. This makes an empty result misleading to the model.

## What Changes

- Report the existing depth and byte scan boundaries when they may omit content.
- Exercise the boundary cases and keep the existing limits and grants.

## Capabilities

### New Capabilities

- `native-search-completeness`: Bounded search reports when limits can omit results.

### Modified Capabilities

None.

## Impact

Native coding search, its tests, and its model-facing description. No public Rust API or dependencies change.
