# Contributing

Start with the [README](README.md) and [development guide](docs/contributing.md). Use small PRs, clear titles, and tests for changed behavior. Follow the existing formatting, architecture, and accessibility rules. If `openspec/` exists, plan and apply feature or bug fixes through OpenSpec, then sync and archive completed changes.

## Human admission and review

Opening or updating a draft PR does not authorize CI spending. Mark the PR Ready for review when the work is ready for checks.

A same-repository non-draft PR whose author is a current human maintainer or admin with GitHub **maintain** or **admin** permission is admitted automatically. Outsiders and forks are admitted only when a current human maintainer marks the PR Ready. Draft PRs cannot be admitted. A new commit needs a new admission. Check limits and run time are bounded.

After reviewing that commit, a maintainer leaves an approving GitHub review, or adds the `gate:approve` label for self-approve under administrator-enforced required reviews. Trusted automation records that label as a bot approval for the current head and removes the label so it can be re-applied after a new push. The `gate:revoke` label cancels an admitted run and dismisses bot approvals for that head. Do not use Actions rerun to replay an admission.

The required statuses are **Quality** and **Repository standards**, plus one approving GitHub review. Address findings in a new commit. Do not bypass the requirements as an administrator.

## Changes and reporting

Keep documentation and `CHANGELOG.md` current. Update all release metadata together when changing `VERSION`; see [releases](RELEASING.md). Never add keys, tokens, personal data, local logs, or private customer files. Use fake fixtures. Report vulnerabilities through [Security](SECURITY.md), and follow the [Code of Conduct](CODE_OF_CONDUCT.md).
