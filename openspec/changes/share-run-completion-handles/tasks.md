# Tasks

## 1. Exact-run completion API

- [ ] 1.1 Add the exact current-run subscription and clonable handle through the existing watch receiver; include the atomic queued-successor decision in detailed completion while preserving summary-only wait. Verify a public regression rejects wrong identities, observes an internally queued successor only after its blocked run-end observer finishes, preserves the true decision after that fast successor finishes and checks false and legacy-wait equivalence.
- [ ] 1.2 Document completion timing, `None`, cancellation and bounded consumer ownership in the Rust contract, host guide, public source page and changelog; check links and source formatting without claiming publication.

## 2. Acceptance

- [ ] 2.1 Pass focused engine contracts, strict Clippy and the normal full harness gate on this exact source, then bind hosted checks, actual complete reviews and protected publication before consumer pin adoption.

## Workflow follow-up

- Sync the verified requirement and archive after all required acceptance passes.
