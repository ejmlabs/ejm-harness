# Proposal

## Why

A durable run-finished event precedes bounded run-end observers and removal from active admission. Hosts need the real completion barrier for an exact run, including successors started by the harness queue, before admitting dependent work.

## What Changes

- Add an exact-session, exact-run completion subscription through the existing public run handle.
- Allow handles to share the same completion and cancellation state without another task or channel.
- Preserve internal queued successors, cancellation, journal events and driver-drop errors.
- Report the driver's queued-successor decision in the same completion result, so a host cannot miss a fast local successor before admitting dependent work. Keep the existing summary-only wait API unchanged.
- Document the barrier and add a bounded public regression with a blocked run-end observer.

## Capabilities

### New Capabilities

- `run-completion`: Exact-run completion observation for native hosts.

### Modified Capabilities

None. The initialized project has no main engine specification to extend.

## Impact

`ejm-harness` owns the additive Rust API and its existing Tokio watch state. CLI consumers retain their own revocable queues and use public handles; IDE adoption is optional. No protocol, provider, dependency, lock, permission or task scheduler changes are required. Public Rust references and the host guide must explain availability, completion timing and cancellation ownership. Local gates, hosted review, protected publication and consumer adoption remain separate acceptance steps.
