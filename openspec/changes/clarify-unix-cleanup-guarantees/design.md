# Design

## Context

See the proposal. Unix cleanup sends group signals and reaps the direct child; sampled descendants receive a separate best-effort signal. Neither path confirms every descendant has exited.

## Goals / Non-Goals

Make the existing limits explicit wherever cleanup or a retained guard is described. Do not alter the implementation, make Unix containment guarantees, or change Windows behavior.

## Decisions

Describe the attempted actions and the possibility that descendants outlive cleanup, including after a guard is released. Preserve the existing fail-closed capability check for callers that require enforced process-tree cancellation.

## Risks / Trade-offs

This correction does not address the underlying ownership gap. Stronger containment needs a separately designed and validated platform-specific backend.
