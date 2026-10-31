# Proposal

## Why

The executor's documentation implies Unix cleanup confirms all background children have stopped. Its process group and sampling mechanisms only provide best-effort termination, so a caller could rely on a guarantee the code does not provide.

## What Changes

- Correct the executor and native ownership documentation and public API comments to state what cleanup does and does not confirm on Unix.
- Preserve the current process behavior and Windows Job Object description.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This is a documentation correction, not a runtime or specification change.

## Impact

Executor documentation and public Rust comments. No API, dependency, or runtime changes.
