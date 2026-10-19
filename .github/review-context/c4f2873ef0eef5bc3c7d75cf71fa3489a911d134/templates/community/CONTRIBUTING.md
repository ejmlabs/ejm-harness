# Contributing

Start with the [README](README.md) and [development guide]({{development_guide}}). Use small PRs, clear titles, and tests for changed behavior. Follow the existing formatting, architecture, and accessibility rules. If `openspec/` exists, plan and apply feature or bug fixes through OpenSpec, then sync and archive completed changes.

## Human admission and review

Opening or updating a PR does not authorize CI or AI spending. A current human maintainer with GitHub **maintain** or **admin** permission posts `/ready FULL_HEAD_SHA` as a standalone PR comment after checking the scope and proposed workflow changes. Use the full current 40-character commit SHA. Draft PRs cannot be admitted. A new commit needs a new decision. Check limits and run time are bounded; repeated events do not authorize another review.

After reviewing that commit, a maintainer posts `/approve FULL_HEAD_SHA`. Maintainers may explicitly approve their own work. GitHub cannot record an author's native approval, so trusted automation records the human comment as a bot approval and a required **Maintainer approval** status. The comment and all checks remain required. Native review alone does not replace this explicit gate. Keep both readiness and approval comments unchanged. Editing or deleting one revokes the decisions. `/revoke FULL_HEAD_SHA` cancels the admitted run and blocks merge. To retry a failed run, revoke, investigate, and post a fresh ready comment. Do not use Actions rerun to replay an admission.

The required statuses are **Human readiness**, **Maintainer approval**, **Quality**, **Repository standards**, and **AI review**. AI review uses `gpt-6.1-sol`, medium reasoning, in two independent passes. A missing key, incomplete output, or blocking finding fails the gate. Address findings in a new commit and request fresh admission. Do not bypass the requirements as an administrator.

## Changes and reporting

Keep documentation and `CHANGELOG.md` current. Update all release metadata together when changing `VERSION`; see [releases](RELEASING.md). Never add keys, tokens, personal data, local logs, or private customer files. Use fake fixtures. Report vulnerabilities through [Security](SECURITY.md), and follow the [Code of Conduct](CODE_OF_CONDUCT.md).
