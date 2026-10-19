# Changelog

Changes follow [Semantic Versioning](https://semver.org/). Each repository is versioned independently.

## [Unreleased]

### Added

- Add exact current-run completion subscriptions and clonable run handles for native hosts, including internally queued successors. Detailed completion also preserves the atomic local-successor decision; summary-only wait stays compatible. Waiting includes bounded run-end observers; the source revision still requires its local gate, hosted review and protected publication.

- Add the encrypted credential vault and controlled registry-session migration APIs.
- Add official Claude Code authentication and supervised structured inference.
- Add optional provider readiness and idle provider replacement with durable continuation binding.
- The resumed-clock deadline fixture and normal local hook passed at `90aa7350e30eaad908d14e8b3260610d5f15f2fe`. Hosted review then found a registry test that required a real OS key service and a legacy FIFO open that could block before validation. Registry-corrected head `86677bbf89f34275614963eb26b91e05fcb7c80b` passed hosted Linux, macOS, registry and standards jobs at the retained snapshot, while Windows was still running; its first complete AI review found the auth cleanup defect below. Native acceptance and release publication remain pending.

- Shared repository standards, explicit human admission, protected merge gates, two-pass Codex review, and a draft source release pipeline.

### Fixed

- Retire the Unix auth process-group identity after successful termination and reaping so a later owner drop cannot signal a reused group identifier. Pending cleanup, errors and Windows Job ownership retain their existing behavior. The bounded regression and fresh local and hosted acceptance remain pending.

## [0.1.0] - 2026-10-18

- Initial development baseline. This entry documents source version metadata; it does not assert a published release or production readiness.
