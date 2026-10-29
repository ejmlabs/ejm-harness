# Design

## Context

See the proposal. Child processes execute on the real OS while Tokio controls the executor deadline.

## Goals / Non-Goals

The tests should exercise deadline and process-budget behavior independently of fixture startup scheduling. This change does not alter process containment or claim a stronger cleanup guarantee.

## Decisions

Pause Tokio's clock for the deadline test, wait for the subprocess readiness record using a bounded wall-clock loop, then advance the deadline. A longer real timeout would remain vulnerable to overloaded runners. Publish the root and one child while still under the process budget, then spawn the remaining children; this allows the test to verify cleanup even if the monitor cancels the fixture immediately after the budget is exceeded. Separate cancellation fixtures cover ordinary descendant cleanup.

## Risks / Trade-offs

The paused-clock test uses a real-time bound on startup because the child is external to Tokio. It does not prove cleanup on other operating systems.
