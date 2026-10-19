# Changelog

Changes follow [Semantic Versioning](https://semver.org/). Each repository is versioned independently.

## [Unreleased]

### Added

- Add the encrypted credential vault and controlled registry-session migration APIs.
- Add official Claude Code authentication and supervised structured inference.
- Add optional provider readiness and idle provider replacement with durable continuation binding.
- The resumed-clock deadline fixture and normal local hook passed at `90aa7350e30eaad908d14e8b3260610d5f15f2fe`. Hosted review then found a registry test that required a real OS key service and a legacy FIFO open that could block before validation. Their corrections still need local and fresh hosted acceptance; native acceptance and release publication remain pending.

- Shared repository standards, explicit human admission, protected merge gates, two-pass Codex review, and a draft source release pipeline.

## [0.1.0] - 2026-10-18

- Initial development baseline. This entry documents source version metadata; it does not assert a published release or production readiness.
